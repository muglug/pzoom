<?php
interface Supplier {
    public function get(): iterable;
}

class SomeClass {
    protected Supplier|iterable $prop;

    public function __construct(Supplier|iterable $value) {
        $this->prop = $value;
    }

    public function do(): void {
        $var = $this->prop;

        if ($var instanceof Supplier) {
            $var->get();
        }
    }
}
