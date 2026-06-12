<?php
class X {
    /**
     * @var string|null
     */
    public static $b;
}

class Y extends X {
    public static $b = "foo";
}

Y::$b = 5;
