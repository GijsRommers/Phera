# cURL option support

Phera registers the PHP 8.5 option names and numeric aliases. A refused option
emits a warning and returns `false`. PHP validation errors such as a null byte
in a string option still throw where applicable. Changing several options in
one call stops at the first failed option.

| Option | Disposition | Behavior |
| --- | --- | --- |
| `CURLOPT_ABSTRACT_UNIX_SOCKET` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_ACCEPTTIMEOUT_MS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_ACCEPT_ENCODING` | Implemented | Decode gzip, deflate, and Brotli. |
| `CURLOPT_ADDRESS_SCOPE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_ALTSVC` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_ALTSVC_CTRL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_APPEND` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_AUTOREFERER` | Implemented | Redirect policy and cross-origin credential rules. |
| `CURLOPT_AWS_SIGV4` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_BINARYTRANSFER` | Accepted and ignored | All transfers are binary-safe. |
| `CURLOPT_BUFFERSIZE` | Implemented | HTTP status, file time, transfer-size limit, and read buffer options. |
| `CURLOPT_CAINFO` | Implemented | Set PEM trust roots for TLS. |
| `CURLOPT_CAINFO_BLOB` | Implemented | Load trust roots from PEM bytes. This takes precedence over `CAINFO`. |
| `CURLOPT_CAPATH` | Implemented | Add PEM trust roots from a directory indexed by `openssl rehash`. |
| `CURLOPT_CA_CACHE_TIMEOUT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_CERTINFO` | Accepted and ignored | False keeps the default. Capturing the certificates is refused. |
| `CURLOPT_CONNECTTIMEOUT` | Implemented | Transfer, connection, and minimum rate deadlines. |
| `CURLOPT_CONNECTTIMEOUT_MS` | Implemented | Transfer, connection, and minimum rate deadlines. |
| `CURLOPT_CONNECT_ONLY` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_CONNECT_TO` | Implemented | Override socket routing. |
| `CURLOPT_COOKIE` | Implemented | Netscape cookie storage. |
| `CURLOPT_COOKIEFILE` | Implemented | Netscape cookie storage. |
| `CURLOPT_COOKIEJAR` | Implemented | Netscape cookie storage. |
| `CURLOPT_COOKIELIST` | Implemented | Netscape cookie storage. |
| `CURLOPT_COOKIESESSION` | Implemented | Netscape cookie storage. |
| `CURLOPT_CRLF` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_CRLFILE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_CUSTOMREQUEST` | Implemented | Method and body selection. |
| `CURLOPT_DEBUGFUNCTION` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DEFAULT_PROTOCOL` | Implemented | HTTP and HTTPS destinations. |
| `CURLOPT_DIRLISTONLY` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DISALLOW_USERNAME_IN_URL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_CACHE_TIMEOUT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_INTERFACE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_LOCAL_IP4` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_LOCAL_IP6` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_SERVERS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_SHUFFLE_ADDRESSES` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_DNS_USE_GLOBAL_CACHE` | Accepted and ignored | False keeps the default. True is refused. |
| `CURLOPT_DOH_SSL_VERIFYHOST` | Refused | Unavailable TLS policy control. |
| `CURLOPT_DOH_SSL_VERIFYPEER` | Refused | Unavailable TLS policy control. |
| `CURLOPT_DOH_SSL_VERIFYSTATUS` | Refused | Unavailable TLS policy control. |
| `CURLOPT_DOH_URL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_EGDSOCKET` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_ENCODING` | Implemented | Decode gzip, deflate, and Brotli. |
| `CURLOPT_EXPECT_100_TIMEOUT_MS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_FAILONERROR` | Implemented | HTTP status, file time, transfer-size limit, and read buffer options. |
| `CURLOPT_FILE` | Implemented | Stream body and headers to PHP output, callbacks, files, or returned bytes. |
| `CURLOPT_FILETIME` | Implemented | HTTP status, file time, transfer-size limit, and read buffer options. |
| `CURLOPT_FNMATCH_FUNCTION` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_FOLLOWLOCATION` | Implemented | Redirect policy and cross-origin credential rules. |
| `CURLOPT_FORBID_REUSE` | Implemented | Control pool use and its capped idle connection count. |
| `CURLOPT_FRESH_CONNECT` | Implemented | Control pool use and its capped idle connection count. |
| `CURLOPT_FTPAPPEND` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTPLISTONLY` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTPPORT` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTPSSLAUTH` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_ACCOUNT` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_ALTERNATIVE_TO_USER` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_CREATE_MISSING_DIRS` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_FILEMETHOD` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_RESPONSE_TIMEOUT` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_SKIP_PASV_IP` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_SSL` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_SSL_CCC` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_USE_EPRT` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_USE_EPSV` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_FTP_USE_PRET` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_GSSAPI_DELEGATION` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HAPPY_EYEBALLS_TIMEOUT_MS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HAPROXYPROTOCOL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HEADER` | Implemented | Stream body and headers to PHP output, callbacks, files, or returned bytes. |
| `CURLOPT_HEADERFUNCTION` | Implemented | Stream body and headers to PHP output, callbacks, files, or returned bytes. |
| `CURLOPT_HEADEROPT` | Accepted and ignored | Accepts CURLHEADER_SEPARATE. Unified proxy headers are refused. |
| `CURLOPT_HSTS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HSTS_CTRL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HTTP09_ALLOWED` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HTTP200ALIASES` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_HTTPAUTH` | Implemented | Basic and Bearer. Other authentication schemes are refused. |
| `CURLOPT_HTTPGET` | Implemented | Method and body selection. |
| `CURLOPT_HTTPHEADER` | Implemented | Outbound headers. |
| `CURLOPT_HTTPPROXYTUNNEL` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_HTTP_CONTENT_DECODING` | Implemented | Decode gzip, deflate, and Brotli. |
| `CURLOPT_HTTP_TRANSFER_DECODING` | Accepted and ignored | True keeps Hyper transfer framing. Disabling it is refused. |
| `CURLOPT_HTTP_VERSION` | Implemented | HTTP/1.0, HTTP/1.1, HTTP/2 with ALPN or prior knowledge. HTTP/3 is refused. |
| `CURLOPT_IGNORE_CONTENT_LENGTH` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_INFILE` | Implemented | Stream upload data from a PHP stream. |
| `CURLOPT_INFILESIZE` | Implemented | Declare the expected upload size. |
| `CURLOPT_INFILESIZE_LARGE` | Implemented | Declare the expected upload size. |
| `CURLOPT_INTERFACE` | Refused | Explicit interface binding is unavailable. |
| `CURLOPT_IPRESOLVE` | Implemented | Override socket routing. |
| `CURLOPT_ISSUERCERT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_ISSUERCERT_BLOB` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_KEEP_SENDING_ON_ERROR` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_KEYPASSWD` | Implemented | Password for an encrypted PKCS#8 private key. |
| `CURLOPT_KRB4LEVEL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_KRBLEVEL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_LOCALPORT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_LOCALPORTRANGE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_LOGIN_OPTIONS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_LOW_SPEED_LIMIT` | Implemented | Transfer, connection, and minimum rate deadlines. |
| `CURLOPT_LOW_SPEED_TIME` | Implemented | Transfer, connection, and minimum rate deadlines. |
| `CURLOPT_MAIL_AUTH` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_MAIL_FROM` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_MAIL_RCPT` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_MAIL_RCPT_ALLLOWFAILS` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_MAXAGE_CONN` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_MAXCONNECTS` | Implemented | Control pool use and its capped idle connection count. |
| `CURLOPT_MAXFILESIZE` | Implemented | HTTP status, file time, transfer-size limit, and read buffer options. |
| `CURLOPT_MAXFILESIZE_LARGE` | Implemented | HTTP status, file time, transfer-size limit, and read buffer options. |
| `CURLOPT_MAXLIFETIME_CONN` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_MAXREDIRS` | Implemented | Redirect policy and cross-origin credential rules. |
| `CURLOPT_MAX_RECV_SPEED_LARGE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_MAX_SEND_SPEED_LARGE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_MIME_OPTIONS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_NETRC` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_NETRC_FILE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_NEW_DIRECTORY_PERMS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_NEW_FILE_PERMS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_NOBODY` | Implemented | Method and body selection. |
| `CURLOPT_NOPROGRESS` | Implemented | Report transfer progress to PHP. |
| `CURLOPT_NOPROXY` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_NOSIGNAL` | Accepted and ignored | Tokio does not use process signals for transfers. |
| `CURLOPT_PASSWORD` | Implemented | Basic and Bearer. Other authentication schemes are refused. |
| `CURLOPT_PATH_AS_IS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_PINNEDPUBLICKEY` | Implemented | Check one or more semicolon-separated `sha256//` leaf public-key pins. File-based pins are refused. |
| `CURLOPT_PIPEWAIT` | Accepted and ignored | True uses Hyper pooling. False is refused. |
| `CURLOPT_PORT` | Implemented | HTTP and HTTPS destinations. |
| `CURLOPT_POST` | Implemented | Method and body selection. |
| `CURLOPT_POSTFIELDS` | Implemented | Binary strings and multipart arrays, including both upload classes. |
| `CURLOPT_POSTQUOTE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_POSTREDIR` | Implemented | Redirect policy and cross-origin credential rules. |
| `CURLOPT_PREQUOTE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_PREREQFUNCTION` | Accepted and ignored | A null callback is accepted. Installing a callback is refused. |
| `CURLOPT_PRE_PROXY` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PRIVATE` | Implemented | Store PHP data, attach a share, and require safe uploads. |
| `CURLOPT_PROGRESSFUNCTION` | Implemented | Report transfer progress to PHP. |
| `CURLOPT_PROTOCOLS` | Implemented | Limit HTTP and HTTPS. No setting enables another scheme. |
| `CURLOPT_PROTOCOLS_STR` | Implemented | Limit HTTP and HTTPS. No setting enables another scheme. |
| `CURLOPT_PROXY` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_PROXYAUTH` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_PROXYHEADER` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_PROXYPASSWORD` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_PROXYPORT` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_PROXYTYPE` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_PROXYUSERNAME` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_PROXYUSERPWD` | Implemented | HTTP forwarding and CONNECT with optional Basic proxy auth. Other types are refused. |
| `CURLOPT_PROXY_CAINFO` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_CAINFO_BLOB` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_CAPATH` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_CRLFILE` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_ISSUERCERT` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_ISSUERCERT_BLOB` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_KEYPASSWD` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_PINNEDPUBLICKEY` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SERVICE_NAME` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLCERT` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLCERTTYPE` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLCERT_BLOB` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLKEY` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLKEYTYPE` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLKEY_BLOB` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSLVERSION` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSL_CIPHER_LIST` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSL_OPTIONS` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSL_VERIFYHOST` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_SSL_VERIFYPEER` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_TLS13_CIPHERS` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_TLSAUTH_PASSWORD` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_TLSAUTH_TYPE` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_TLSAUTH_USERNAME` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PROXY_TRANSFER_MODE` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_PUT` | Implemented | Method and body selection. |
| `CURLOPT_QUICK_EXIT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_QUOTE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_RANDOM_FILE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_RANGE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_READDATA` | Implemented | Read PHP stream or callback before network dispatch. |
| `CURLOPT_READFUNCTION` | Implemented | Request bounded chunks from a PHP callback while sending. |
| `CURLOPT_REDIR_PROTOCOLS` | Implemented | Limit HTTP and HTTPS. No setting enables another scheme. |
| `CURLOPT_REDIR_PROTOCOLS_STR` | Implemented | Limit HTTP and HTTPS. No setting enables another scheme. |
| `CURLOPT_REFERER` | Implemented | Outbound headers. |
| `CURLOPT_REQUEST_TARGET` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_RESOLVE` | Implemented | Override socket routing. |
| `CURLOPT_RESUME_FROM` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_RETURNTRANSFER` | Implemented | Stream body and headers to PHP output, callbacks, files, or returned bytes. |
| `CURLOPT_RTSP_CLIENT_CSEQ` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_RTSP_REQUEST` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_RTSP_SERVER_CSEQ` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_RTSP_SESSION_ID` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_RTSP_STREAM_URI` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_RTSP_TRANSPORT` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SAFE_UPLOAD` | Implemented | Store PHP data, attach a share, and require safe uploads. |
| `CURLOPT_SASL_AUTHZID` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_SASL_IR` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_SERVER_RESPONSE_TIMEOUT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_SERVICE_NAME` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_SHARE` | Implemented | Store PHP data, attach a share, and require safe uploads. |
| `CURLOPT_SOCKS5_AUTH` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_SOCKS5_GSSAPI_NEC` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_SOCKS5_GSSAPI_SERVICE` | Refused | Unavailable proxy option. The HTTP proxy scope is documented above. |
| `CURLOPT_SSH_AUTH_TYPES` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_COMPRESSION` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_HOSTKEYFUNCTION` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_HOST_PUBLIC_KEY_MD5` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_HOST_PUBLIC_KEY_SHA256` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_KNOWNHOSTS` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_PRIVATE_KEYFILE` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSH_PUBLIC_KEYFILE` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_SSLCERT` | Implemented | Load the client certificate and optional chain from a file. |
| `CURLOPT_SSLCERTPASSWD` | Implemented | Alias of `KEYPASSWD`. |
| `CURLOPT_SSLCERTTYPE` | Implemented | Select PEM or DER. PKCS#12 and engine-backed identities are refused. |
| `CURLOPT_SSLCERT_BLOB` | Implemented | Load the client certificate and optional chain from bytes. |
| `CURLOPT_SSLENGINE` | Refused | The rustls client has no SSL engine. |
| `CURLOPT_SSLENGINE_DEFAULT` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSLKEY` | Implemented | Load the client private key from a file. |
| `CURLOPT_SSLKEYPASSWD` | Implemented | Alias of `KEYPASSWD`. |
| `CURLOPT_SSLKEYTYPE` | Implemented | Select PEM or DER. Engine-backed keys are refused. |
| `CURLOPT_SSLKEY_BLOB` | Implemented | Load the client private key from bytes. |
| `CURLOPT_SSLVERSION` | Implemented | TLS 1.2 and 1.3. Other protocol versions are refused. |
| `CURLOPT_SSL_CIPHER_LIST` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_EC_CURVES` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_ENABLE_ALPN` | Implemented | Select ALPN and require verification. Disabling certificate or hostname checks is refused. |
| `CURLOPT_SSL_ENABLE_NPN` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_FALSESTART` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_OPTIONS` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_SESSIONID_CACHE` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_SIGNATURE_ALGORITHMS` | Refused | Unavailable TLS policy control. |
| `CURLOPT_SSL_VERIFYHOST` | Implemented | Match the peer identity to the URL hostname by default. Zero disables this check. |
| `CURLOPT_SSL_VERIFYPEER` | Implemented | Validate the certificate chain against trusted roots by default. False disables this check. |
| `CURLOPT_SSL_VERIFYSTATUS` | Refused | Unavailable TLS policy control. |
| `CURLOPT_STDERR` | Implemented | Write a Phera trace to stderr or a PHP stream. |
| `CURLOPT_STREAM_WEIGHT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_SUPPRESS_CONNECT_HEADERS` | Accepted and ignored | True keeps proxy handshake headers internal. False is refused. |
| `CURLOPT_TCP_FASTOPEN` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_TCP_KEEPALIVE` | Implemented | Configure the TCP socket. |
| `CURLOPT_TCP_KEEPCNT` | Implemented | Configure the TCP socket. |
| `CURLOPT_TCP_KEEPIDLE` | Implemented | Configure the TCP socket. |
| `CURLOPT_TCP_KEEPINTVL` | Implemented | Configure the TCP socket. |
| `CURLOPT_TCP_NODELAY` | Implemented | Configure the TCP socket. |
| `CURLOPT_TELNETOPTIONS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_TFTP_BLKSIZE` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_TFTP_NO_OPTIONS` | Refused | Protocol outside the HTTP scope. |
| `CURLOPT_TIMECONDITION` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_TIMEOUT` | Implemented | Transfer, connection, and minimum rate deadlines. |
| `CURLOPT_TIMEOUT_MS` | Implemented | Transfer, connection, and minimum rate deadlines. |
| `CURLOPT_TIMEVALUE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_TIMEVALUE_LARGE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_TLS13_CIPHERS` | Refused | Unavailable TLS policy control. |
| `CURLOPT_TLSAUTH_PASSWORD` | Refused | Unavailable TLS policy control. |
| `CURLOPT_TLSAUTH_TYPE` | Refused | Unavailable TLS policy control. |
| `CURLOPT_TLSAUTH_USERNAME` | Refused | Unavailable TLS policy control. |
| `CURLOPT_TRANSFERTEXT` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_TRANSFER_ENCODING` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_UNIX_SOCKET_PATH` | Implemented | Override socket routing. |
| `CURLOPT_UNRESTRICTED_AUTH` | Implemented | Redirect policy and cross-origin credential rules. |
| `CURLOPT_UPKEEP_INTERVAL_MS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_UPLOAD` | Implemented | Method and body selection. |
| `CURLOPT_UPLOAD_BUFFERSIZE` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_URL` | Implemented | HTTP and HTTPS destinations. |
| `CURLOPT_USERAGENT` | Implemented | Outbound headers. |
| `CURLOPT_USERNAME` | Implemented | Basic and Bearer. Other authentication schemes are refused. |
| `CURLOPT_USERPWD` | Implemented | Basic and Bearer. Other authentication schemes are refused. |
| `CURLOPT_USE_SSL` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_VERBOSE` | Implemented | Write a Phera trace to stderr or a PHP stream. |
| `CURLOPT_WILDCARDMATCH` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_WRITEFUNCTION` | Implemented | Stream body and headers to PHP output, callbacks, files, or returned bytes. |
| `CURLOPT_WRITEHEADER` | Implemented | Stream body and headers to PHP output, callbacks, files, or returned bytes. |
| `CURLOPT_WS_OPTIONS` | Refused | Unavailable in the HTTP client. |
| `CURLOPT_XFERINFOFUNCTION` | Implemented | Report transfer progress to PHP. |
| `CURLOPT_XOAUTH2_BEARER` | Implemented | Basic and Bearer. Other authentication schemes are refused. |

`CURLINFO_HEADER_OUT` is also accepted by `curl_setopt()`. It enables a
reconstructed request header for `curl_getinfo()`.

Multi handles accept `CURLMOPT_PIPELINING`, `CURLMOPT_MAX_HOST_CONNECTIONS`,
`CURLMOPT_MAX_TOTAL_CONNECTIONS`, and `CURLMOPT_MAXCONNECTS`.
`CURLMOPT_PUSHFUNCTION` and other multi options are refused.
