<?php
/**
 * @psalm-type T string
 *
 * Lorem ipsum
 */
class Foo {
    /** @psalm-var T */
    public static string $t;
}
$t = Foo::$t;
