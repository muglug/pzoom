<?php
class A {
    public function __set(string $s, mixed $value) : void {}
}

(new A)->__set("foo");
