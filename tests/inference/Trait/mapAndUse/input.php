<?php
class C
{
    use T1 {
        traitFunc as _func;
    }
    use T2;

    public static function func(): void
    {
        static::_func();
    }
}
trait T1
{
    public static function traitFunc(): void {}
}
trait T2 { }
