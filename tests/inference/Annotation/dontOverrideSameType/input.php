<?php
class A {
    /** @return ?int */
    public function foo(): ?int {
        if (rand(0, 1)) return 5;
    }
}
