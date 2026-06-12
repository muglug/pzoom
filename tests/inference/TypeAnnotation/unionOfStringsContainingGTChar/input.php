<?php
/** @psalm-type T '<'|'>' */
class Foo {
    /** @psalm-var T */
    public static string $t;
}
$t = Foo::$t;
