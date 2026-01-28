<?php
trait T {
    public function compare(O $other) : void {
        if ($other instanceof self) {
            if ($other->value === $this->value) {}
        }
    }
}

class O {}

class A extends O {
    use T;

    /** @var string */
    private $value;

    public function __construct(string $string) {
       $this->value = $string;
    }
}

class B extends O {
    use T;

    /** @var bool */
    private $value;

    public function __construct(bool $bool) {
       $this->value = $bool;
    }
}
