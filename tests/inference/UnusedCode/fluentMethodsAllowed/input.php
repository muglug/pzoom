<?php
final class A {
    public function foo(): static {
        return $this;
    }

    public function bar(): static {
        return $this;
    }
}

(new A())->foo()->bar();
