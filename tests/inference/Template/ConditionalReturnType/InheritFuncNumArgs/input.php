<?php
abstract class A
{
    /**
     * @psalm-return (func_num_args() is 1 ? string : int)
     */
    public static function get(bool $a, ?bool $b = null)
    {
        if ($b) {
            return 1;
        }
        return "";
    }
}

class B extends A
{

    public static function getB(bool $a): int
    {
        return self::get($a, true);
    }
}
