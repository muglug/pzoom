<?php
class Foo
{
    /** @var int */
    private $status;

    public function __construct(int $in)
    {
        if (rand(0, 1)) {
            $this->status = 1;
        } else {
            $this->status = $in;
        }
    }
}
