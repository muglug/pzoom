<?php
class A
{
    /**
     * @var null|A
     */
    public $parent;

    public static function foo(A $a) : void
    {
        do {
            if ($a->parent === null) {
                throw new \Exception("bad");
            }

            $a = $a->parent;
        } while (rand(0,1));
    }
}
