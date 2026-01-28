<?php
class C
{
    use T2;

    use T1 {
        traitFunc as aliasedTraitFunc;
    }

    public static function func(): void
    {
        static::aliasedTraitFunc();
    }
}
trait T1
{
    public static function traitFunc(): void {}
}
trait T2 { }
