<?php

class Cfg {
    private static ?Cfg $instance = null;
    public int $max = 100;

    public static function getInstance(): Cfg
    {
        return self::$instance ??= new Cfg();
    }
}

/** @psalm-immutable */
class Lit2 {
    public string $value;

    public function __construct(string $value)
    {
        $cfg = Cfg::getInstance();
        if (strlen($value) >= $cfg->max) {
            throw new InvalidArgumentException('too long');
        }
        $this->value = $value;
    }
}
