<?php
class A {
    /** @var int */
    public $i = 1;
}

abstract class B
{
    /**
     * @var string
     */
    protected $foo;

    /**
     * @var A[]
     */
    private $as = [];

    public function __construct()
    {
        $this->foo = "";
        $this->bar();
    }

    public function bar(): void
    {
        \usort($this->as, function (A $a, A $b): int {
            return $b->i <=> $a->i;
        });
    }
}

class C extends B
{
    public function __construct()
    {
        parent::__construct();
    }
}
