<?php
class A {
    public function foo(self $value): void {
        if ($value instanceof static) {}
    }
}