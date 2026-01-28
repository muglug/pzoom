<?php
/**
 * @psalm-consistent-constructor
 */
class Example {
    static function staticMethod(): string {
        return "";
    }

    public function instanceMethod(): string {
        $className = get_class();

        return $className::staticMethod();
    }
}

/**
 * @param class-string<Example> $className
 */
function example(string $className, Example $object): string {
    $objectClassName = get_class($object);

    takesExampleClassString($className);
    takesExampleClassString($objectClassName);

    if (rand(0, 1)) {
        return (new $className)->instanceMethod();
    }

    if (rand(0, 1)) {
        return (new $objectClassName)->instanceMethod();
    }

    if (rand(0, 1)) {
        return $className::staticMethod();
    }

    return $objectClassName::staticMethod();
}


/** @param class-string<Example> $className */
function takesExampleClassString(string $className): void {}
