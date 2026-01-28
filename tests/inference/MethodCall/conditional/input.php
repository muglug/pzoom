<?php

class Old {
    public function x(): void {}
}
class Child1 extends Old {}
class Child2 extends Old {}

/**
 * @template IsClient of bool
 */
class A {
    /**
     * @psalm-param (IsClient is true ? Child1 : Child2) $var
     */
    public function test(Old $var): void {
        $var->x();
    }
}
