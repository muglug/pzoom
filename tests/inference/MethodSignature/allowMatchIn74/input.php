<?php
trait FooTrait {
    /**
     * @return static
     */
    public function bar(): self  {
        return $this;
    }
}

interface FooInterface {
    /**
     * @return static
     */
    public function bar(): self;
}

class FooClass implements FooInterface {
    use FooTrait;
}
