<?php
class A {
    public function __get(string $s) : mixed {}
}

(new A)->__get();
