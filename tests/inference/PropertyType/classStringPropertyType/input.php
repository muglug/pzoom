<?php
class C {
    /** @psalm-var array<class-string, int> */
    public $member = [
        InvalidArgumentException::class => 1,
    ];
}
