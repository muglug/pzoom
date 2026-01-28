<?php
/**
 * @template T as object
 *
 * @psalm-property-read class-string<T> $name
 */
class CustomReflectionClass {
    /**
     * @var class-string<T>
     */
    public $name;

    /**
     * @param T|class-string<T> $argument
     */
    public function __construct($argument) {
        if (is_object($argument)) {
            $this->name = get_class($argument);
        } else {
            $this->name = $argument;
        }
    }
}

/**
 * @template T as object
 * @param class-string<T> $className
 * @return CustomReflectionClass<T>
 */
function getTypeOf(string $className) {
    return new CustomReflectionClass($className);
}