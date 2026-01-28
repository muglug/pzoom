<?php
class X {
    /**
     * @var string|null
     */
    public $a;

    /**
     * @var string|null
     */
    public static $b;
}

class Y extends X {
    public $a = "foo";
    public static $b = "foo";
}

(new Y)->a = "hello";
echo (new Y)->a;
Y::$b = "bar";
echo Y::$b;
