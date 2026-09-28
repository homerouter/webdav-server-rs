#include <security/pam_appl.h>
#include <sys/resource.h>
#include <string.h>
#include <stdlib.h>

struct creds {
    char *user;
    char *password;
};

static int c_pam_conv(int num_msg, const struct pam_message **msg,
                      struct pam_response **resp, void *appdata)
{
    if (num_msg <= 0 || num_msg > PAM_MAX_NUM_MSG)
        return PAM_CONV_ERR;

    struct pam_response *reply = calloc(num_msg, sizeof(struct pam_response));
    if (reply == NULL)
        return PAM_BUF_ERR;

    struct creds *creds = (struct creds *)appdata;

    for (int count = 0; count < num_msg; count++) {
        switch (msg[count]->msg_style) {
            case PAM_PROMPT_ECHO_ON:
                reply[count].resp_retcode = 0;
                reply[count].resp = strdup(creds->user ? creds->user : "");
                break;
            case PAM_PROMPT_ECHO_OFF:
                reply[count].resp_retcode = 0;
                reply[count].resp = strdup(creds->password ? creds->password : "");
                break;
            case PAM_TEXT_INFO:
            case PAM_ERROR_MSG:
                reply[count].resp_retcode = 0;
                reply[count].resp = NULL;
                break;
            default:
                for (int i = 0; i < num_msg; i++) {
                    if (reply[i].resp)
                        free(reply[i].resp);
                }
                free(reply);
                return PAM_CONV_ERR;
        }
    }
    *resp = reply;
    return PAM_SUCCESS;
}

int c_pam_auth(char *service, char *user, char *pass, char *remip)
{
    struct creds creds = {
        user,
        pass,
    };
    struct pam_conv conv = {
        c_pam_conv,
        &creds,
    };

    pam_handle_t *pamh = NULL;
    int ret = pam_start(service, user, &conv, &pamh);
    if (ret != PAM_SUCCESS)
        return ret;
    if (ret == PAM_SUCCESS && remip && remip[0])
        ret = pam_set_item(pamh, PAM_RHOST, remip);
    if (ret == PAM_SUCCESS)
        ret = pam_authenticate(pamh, 0);
    pam_end(pamh, ret);

    return ret;
}

void c_pam_lower_rlimits()
{
    struct rlimit rlim;
    if (getrlimit(RLIMIT_NOFILE, &rlim) == 0) {
        rlim_t l = rlim.rlim_cur;
        if (l > 256)
            l = 256;
        rlim.rlim_cur = l;
        rlim.rlim_max = l;
        setrlimit(RLIMIT_NOFILE, &rlim);
    }
}
