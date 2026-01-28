<?php
/**
 * @template T
 */
class Foo {
    /** @var T */
    public $obj;

    /**
     * @param T $obj
     */
    public function __construct($obj) {
        $this->obj = $obj;
    }

    /**
     * @return T
     */
    public function bar() {
        return $this->obj;
    }

    /**
     * @return T
     */
    public function bat() {
        return $this->bar();
    }

    public function __toString(): string {
        return "hello " . $this->obj;
    }
}