// cloud sync module, was going to pull policies from S3 via API Gateway
// but the feature got descoped before the demo. Leaving this around because
// we'll probably need it for the production rollout in Q3.
// NOTE: not wired into main.rs (no `mod cloud`) so none of this runs.
pub mod auth;
pub mod policies;
