<?php
class A {
    private function __clone() {}
    public function foo(): self {
        return clone $this;
    }
}
