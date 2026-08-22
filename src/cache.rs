use std::borrow::Borrow;
use std::cmp::Eq;
use std::collections::vec_deque::VecDeque;
use std::collections::HashMap;
use std::hash::Hash;
use std::option::Option;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[allow(dead_code)]
pub struct Cache<K, V> {
    intern: Mutex<Intern<K, V>>,
}

struct Intern<K, V> {
    maxsize: usize,
    maxage: Duration,
    map: HashMap<K, Arc<V>>,
    fifo: VecDeque<(Instant, K)>,
}

impl<K: Hash + Eq + Clone, V> Default for Cache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Hash + Eq + Clone, V> Cache<K, V> {
    pub fn new() -> Cache<K, V> {
        let i = Intern {
            maxsize: 0,
            maxage: Duration::new(0, 0),
            map: HashMap::new(),
            fifo: VecDeque::new(),
        };
        Cache {
            intern: Mutex::new(i),
        }
    }

    #[allow(dead_code)]
    pub fn maxsize(self, maxsize: usize) -> Self {
        self.intern.lock().unwrap().maxsize = maxsize;
        self
    }

    #[allow(dead_code)]
    pub fn maxage(self, maxage: Duration) -> Self {
        self.intern.lock().unwrap().maxage = maxage;
        self
    }

    fn expire(&self, m: &mut Intern<K, V>) {
        let mut n = m.fifo.len();
        if m.maxsize > 0 && n >= m.maxsize {
            n = m.maxsize;
        }
        if m.maxage.as_secs() > 0 || m.maxage.subsec_nanos() > 0 {
            let now = Instant::now();
            while n > 0 {
                let &(t, _) = m.fifo.get(n - 1).unwrap();
                if now.duration_since(t) <= m.maxage {
                    break;
                }
                n -= 1;
            }
        }
        for x in n..m.fifo.len() {
            let (_, key) = m.fifo.get(x).unwrap();
            m.map.remove(key);
        }
        m.fifo.truncate(n);
    }

    pub fn insert(&self, key: K, val: V) -> Arc<V> {
        let mut m = self.intern.lock().unwrap();
        self.expire(&mut m);
        let av = Arc::new(val);
        let ac = av.clone();
        m.map.insert(key.clone(), av);
        m.fifo.push_front((Instant::now(), key));
        ac
    }

    // see https://doc.rust-lang.org/book/first-edition/borrow-and-asref.html
    pub fn get<Q>(&self, key: &Q) -> Option<Arc<V>>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let mut m = self.intern.lock().unwrap();
        self.expire(&mut m);
        m.map.get(key).cloned()
    }
}

pub(crate) mod cached {
    //
    // Cached versions of Unix account lookup and Pam auth.
    //
    use std::io;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use crate::cache;
    use crate::unixuser::{self, User};

    struct Timeouts {
        pwcache: Duration,
        #[cfg(feature = "pam")]
        pamcache: Duration,
    }

    static TIMEOUTS: OnceLock<Mutex<Timeouts>> = OnceLock::new();
    static PWCACHE: OnceLock<cache::Cache<String, unixuser::User>> = OnceLock::new();
    #[cfg(feature = "pam")]
    static PAMCACHE: OnceLock<cache::Cache<u64, String>> = OnceLock::new();

    fn get_timeouts() -> &'static Mutex<Timeouts> {
        TIMEOUTS.get_or_init(|| {
            Mutex::new(Timeouts {
                pwcache: Duration::from_secs(120),
                #[cfg(feature = "pam")]
                pamcache: Duration::from_secs(120),
            })
        })
    }

    fn get_pwcache() -> &'static cache::Cache<String, unixuser::User> {
        PWCACHE.get_or_init(|| {
            let timeouts = get_timeouts().lock().unwrap();
            cache::Cache::new().maxage(timeouts.pwcache)
        })
    }

    #[cfg(feature = "pam")]
    fn get_pamcache() -> &'static cache::Cache<u64, String> {
        PAMCACHE.get_or_init(|| {
            let timeouts = get_timeouts().lock().unwrap();
            cache::Cache::new().maxage(timeouts.pamcache)
        })
    }

    pub(crate) fn set_pwcache_timeout(secs: usize) {
        let mut timeouts = get_timeouts().lock().unwrap();
        timeouts.pwcache = Duration::from_secs(secs as u64);
    }

    #[cfg(feature = "pam")]
    pub(crate) fn set_pamcache_timeout(secs: usize) {
        let mut timeouts = get_timeouts().lock().unwrap();
        timeouts.pamcache = Duration::from_secs(secs as u64);
    }

    #[cfg(feature = "pam")]
    pub async fn pam_auth<'a>(
        pam_auth: pam_sandboxed::PamAuth,
        service: &'a str,
        user: &'a str,
        pass: &'a str,
        remip: Option<&'a str>,
    ) -> Result<(), pam_sandboxed::PamError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut s = DefaultHasher::new();
        service.hash(&mut s);
        user.hash(&mut s);
        pass.hash(&mut s);
        remip.as_ref().hash(&mut s);
        let key = s.finish();

        if let Some(cache_user) = get_pamcache().get(&key) {
            if user == cache_user.as_str() {
                return Ok(());
            }
        }

        let mut pam_auth = pam_auth;
        match pam_auth.auth(service, user, pass, remip).await {
            Err(e) => Err(e),
            Ok(()) => {
                get_pamcache().insert(key, user.to_owned());
                Ok(())
            },
        }
    }

    pub async fn unixuser(username: &str, with_groups: bool) -> Result<Arc<User>, io::Error> {
        let pwcache = get_pwcache();
        if let Some(pwd) = pwcache.get(username) {
            return Ok(pwd);
        }
        match User::by_name_async(username, with_groups).await {
            Err(e) => Err(e),
            Ok(pwd) => Ok(pwcache.insert(username.to_owned(), pwd)),
        }
    }
}
