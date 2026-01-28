<?php
namespace NS;

class Base
{
    /**
     * @var int
     */
    protected $a;

    final private function __construct()
    {
        $this->setA();
    }

    private function setA() : void {
        $this->a = 5;
    }

    public static function getInstance(): self { return new static; }
}

class Concrete extends Base {}
