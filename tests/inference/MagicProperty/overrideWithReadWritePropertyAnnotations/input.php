<?php
namespace Bar;

/**
 * @psalm-property int $foo
 * @property-read string $foo
 * @property-write array $foo
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

    public function takesString(string $s): void {}
}

$a = new A();
$a->foo = [];

$a = new A();
$a->takesString($a->foo);
