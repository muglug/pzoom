<?php
class X {
    /**
     * @var string|null
     */
    public $a;
}

class Y extends X {
    public $a = "foo";
}

(new Y)->a = 5;
echo (new Y)->a;
Y::$b = "bar";
echo Y::$b;
