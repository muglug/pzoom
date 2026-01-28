<?php
namespace Bar;

class Foo {
    /** @var class-string<self> */
    private static $c;

    /**
     * @return class-string<self>
     */
    public static function r() : string
    {
        return self::$c;
    }
}
