/* Unit test for the PAM conversation helpers. Built and run in CI:
 *   cc -I pam/src tests/pam_memzero.c -lpam -o /tmp/pam_memzero && /tmp/pam_memzero
 * pam.c is included directly so the static secure_memzero() is reachable. */
#include "../pam/src/pam.c"
#include <assert.h>
#include <stdio.h>

int main(void)
{
    /* secure_memzero wipes every byte, including past an embedded NUL. */
    unsigned char buf[16];
    memset(buf, 0xAA, sizeof buf);
    buf[3] = 0;
    secure_memzero(buf, sizeof buf);
    for (size_t i = 0; i < sizeof buf; i++)
        assert(buf[i] == 0);

    /* The conversation function hands back copies of user/password for the right prompts... */
    struct creds creds = { "alice", "s3cret" };
    struct pam_message m1 = { PAM_PROMPT_ECHO_ON, "login:" };
    struct pam_message m2 = { PAM_PROMPT_ECHO_OFF, "Password:" };
    const struct pam_message *msgs[2] = { &m1, &m2 };
    struct pam_response *resp = NULL;
    assert(c_pam_conv(2, msgs, &resp, &creds) == PAM_SUCCESS);
    assert(strcmp(resp[0].resp, "alice") == 0);
    assert(strcmp(resp[1].resp, "s3cret") == 0);
    /* The password copy must be a separate allocation from the caller's buffer. */
    assert(resp[1].resp != creds.password);
    free(resp[0].resp);
    free(resp[1].resp);
    free(resp);

    /* ...and fails cleanly (no response, no leak) on an unknown message style. */
    struct pam_message bad = { 99, "?" };
    const struct pam_message *badmsgs[2] = { &m2, &bad };
    resp = (struct pam_response *)0x1;
    assert(c_pam_conv(2, badmsgs, &resp, &creds) == PAM_CONV_ERR);
    assert(resp == NULL);

    puts("pam_memzero: ok");
    return 0;
}
