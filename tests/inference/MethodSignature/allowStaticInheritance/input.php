<?php
class A {
    public function method(): static {
        return $this;
    }
}
class B extends A {
    public function method(): static {
        return $this;
    }
}
