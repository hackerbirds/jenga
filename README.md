### jenga

`jenga` aims to achieve the same as the `tower` crate in Rust, but with a simpler and modern API that makes use of newer Rust features. Notably, Future isn't manually handled anymore.

!! THIS IS A HOBBY WIP !! 

### middlewares available

Activate the feature flags to use the middlewares you want.

- `timeout`: waits N seconds for request to finish, it not then it times out. relies on Tokio for async timer
- `retry`: retries the request N times before failing. instant with no waiting in between
- `retry_wait`: adds the ability on `retry` to wait between retries. relies on Tokio for async timer, which is why it's behind a feature flag.
- `rate_limit`: allows up to N concurrent requests from being processed at the same time.
- `restart`: restart a service automatically if it returns an error, using a generator service