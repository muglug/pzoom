<?php
class Foo
{
    /**
     * @psalm-var "a"|"b"
     */
    private $s;

    public function __construct(string $s)
    {
        if (!in_array($s, ["a", "b"], true)) {
            throw new \InvalidArgumentException;
        }
        $this->s = $s;
    }
}