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

    public function redirectToA() : void {
        B::setRedirectHeader($this->getA());
    }
}

class B {
    public static function setRedirectHeader(string $s) : void {
        header("Location: $s");
    }
}
