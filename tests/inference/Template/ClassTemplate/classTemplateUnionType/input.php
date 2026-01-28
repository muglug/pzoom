<?php
/**
 * @template T0 as int|string
 */
class C {
    /**
     * @param T0 $t
     */
    public function foo($t) : void {}
}

/** @param C<int> $c */
function foo(C $c) : void {}

/** @param C<string> $c */
function bar(C $c) : void {}