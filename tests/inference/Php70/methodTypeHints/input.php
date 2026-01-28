<?php
class Foo {
    public static function indexof(string $haystack, string $needle): int
    {
        $pos = strpos($haystack, $needle);

        if ($pos === false) {
            return -1;
        }

        return $pos;
    }
}

$a = Foo::indexof("arr", "a");
