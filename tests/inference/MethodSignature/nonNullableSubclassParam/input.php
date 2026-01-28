<?php
class A {
    public function foo(?string $s): string {
        return $s !== null ? $s : "hello";
    }
}

class B extends A {
    public function foo(string $s): string {
        return $s;
    }
}
