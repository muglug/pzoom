<?php
/**
 * @property string $test
 */
class A {
    public function __get(string $name) {}

    /**
     * @param mixed $value
     */
    public function __set(string $name, $value) {}
}

class B extends A {
    public function test(): string {
        return $this->__get("test");
    }
}
