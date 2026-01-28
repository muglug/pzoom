<?php
class A {
    public function __construct(
        /**
         * @psalm-readonly-allow-private-mutation
         */
        public ?string $bar = null,
    ) {}

    public function setBar(string $s) : void {
        $this->bar = $s;
    }
}

echo (new A)->bar;
