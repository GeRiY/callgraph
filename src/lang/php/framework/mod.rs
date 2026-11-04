//! Framework-felismerés és -stratégiák (Strategy + Factory).

mod detector;
mod generic;
mod laravel;
mod strategy;
mod yii;

pub use detector::FrameworkDetector;
pub use generic::GenericStrategy;
pub use laravel::LaravelStrategy;
pub use strategy::FrameworkStrategy;
pub use yii::YiiStrategy;
