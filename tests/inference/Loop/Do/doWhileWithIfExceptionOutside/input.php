<?php
class A
{
    /**
     * @var null|A
     */
    public $parent;

    public static function foo(A $a) : void
    {
        if ($a->parent === null) {
            throw new \Exception("bad");
        }

        do {
            $a = $a->parent;
        } while ($a->parent && rand(0, 1));
    }
}
