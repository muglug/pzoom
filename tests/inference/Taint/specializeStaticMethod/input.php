<?php
StringUtility::foo($_GET["c"]);

class StringUtility {
    /**
     * @psalm-taint-specialize
     */
    public static function foo(string $str) : string
    {
        return $str;
    }

    /**
     * @psalm-taint-specialize
     */
    public static function slugify(string $url) : string {
        return self::foo($url);
    }
}

echo StringUtility::slugify("hello");
