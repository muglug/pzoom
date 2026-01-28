<?php
class A {
    /**
     * @var mixed
     */
    public $foo = "hello";

    /** @return void */
    public function barBar()
    {
        if (rand(0,10) === 5) {
            $this->foo = [];
        }

        if (!is_array($this->foo)) {
            // do something
        }
    }
}
