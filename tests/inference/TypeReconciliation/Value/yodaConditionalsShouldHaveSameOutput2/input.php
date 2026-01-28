<?php
class Foo {
    /**
     * @var array{from:bool, to:bool}
     */
    protected $things = ["from" => false, "to" => false];

    public function foo(string ...$things) : void {
        foreach ($things as $thing) {
            if ("from" !== $thing && "to" !== $thing) {
                continue;
            }

            $this->things[$thing] = !$this->things[$thing];
        }
    }
}
                