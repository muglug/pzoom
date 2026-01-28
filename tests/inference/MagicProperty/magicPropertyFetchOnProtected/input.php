<?php
/** @psalm-no-seal-properties */
class C {
    /** @var string */
    protected $foo = "foo";

    public function __get(string $name) {}

    /**
     * @param mixed $value
     */
    public function __set(string $name, $value)
    {
    }
}

$c = new C();
$c->foo = "bar";
echo $c->foo;
