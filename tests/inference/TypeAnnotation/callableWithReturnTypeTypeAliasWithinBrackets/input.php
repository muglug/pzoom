<?php
/** @psalm-type TCallback (callable():int) */
class Foo {
    /** @psalm-var TCallback */
    public static callable $callback;
}
$output = Foo::$callback;
