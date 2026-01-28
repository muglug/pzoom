<?php
class A {
    /** @var self|null */
    public static $instance;

    /** @var string|null */
    public $bat;

    public function foo() : void {
        if (self::$instance) {
            self::$instance->bar();
            echo self::$instance->bat;
        }
    }

    public function bar() : void {}
}

if (A::$instance) {
    A::$instance->bar();
    echo A::$instance->bat;
}
