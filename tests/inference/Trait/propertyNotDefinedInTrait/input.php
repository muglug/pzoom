<?php
class A1 {
    use A2;

    public static string $titlefield = "blah";
}

trait A2 {
    public static function test() : string {
        /**
         * @var string
         */
        $sortfield = (isset(static::$sortfield)) ?
                    static::$sortfield
                    : static::$titlefield;
        return $sortfield;
    }
}
