<?php
namespace Bar;

class Foo {}
class A {
    /** @var array<string, object> */
    public array $map = [];

    /**
     * @param string $class
     */
    public function get(string $class) : void {
        $this->map[$class] = 5;
    }
}
