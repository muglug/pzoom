<?php
/**
 * @psalm-immutable
 */
class A {
    /** @var string */
    public $a;

    public function __construct(string $a) {
        $this->a = $a;
    }

    public function getA() : string {
        return $this->a;
    }

    public function getHelloA() : string {
        return "hello" . $this->getA();
    }
}
