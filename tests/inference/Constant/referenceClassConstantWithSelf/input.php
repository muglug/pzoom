<?php
abstract class A {
    /** @var array<non-empty-string, non-empty-string> */
    public const KEYS = [];
    /** @var array<non-empty-string, non-empty-string> */
    public const VALUES = [];
}

class B extends A {
    public const VALUES = ['there' => self::KEYS['hi']];
    public const KEYS = ['hi' => CONSTANTS::THERE];
}

class CONSTANTS {
    public const THERE = 'there';
}

echo B::VALUES["there"];
