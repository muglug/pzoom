<?php

class A {
    /** @var array<class-string, string> */
    private array $a;

    public function __construct() {
        $this->a = [];
    }

    public function handle(): void {
        $b = [A::class => "d"];
        $this->a = array_merge($this->a, $b);
    }
}
                    
