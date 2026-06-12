<?php
final class A {
    /** @var int */
    private $used;

    /** @var int */
    private $unused;

    /** @var int */
    private static $staticUnused;

    public function __construct() {
        $this->used = 4;
        $this->unused = 4;
        self::$staticUnused = 4;
    }

    public function handle(): void
    {
        $this->used++;
    }
}
(new A())->handle();
