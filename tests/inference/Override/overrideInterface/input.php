<?php
interface I {
    public function f(): void;
}

interface I2 extends I {
    #[Override]
    public function f(): void;
}
