<?php
namespace Bar;

class PropertyType {}

/**
 * @property PropertyType $foo
 */
class A {
    /** @param string $name */
    public function __get($name): ?string {
        if ($name === "foo") {
            return "hello";
        }

        return null;
    }

    /**
     * @param string $name
     * @param mixed $value
     */
    public function __set($name, $value): void {
    }
}

$a = new A();
$a->foo = new PropertyType();
