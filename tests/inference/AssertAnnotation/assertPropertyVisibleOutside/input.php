<?php
class A {
    public ?int $x = null;

    public function maybeAssignX() : void {
        if (rand(0, 0) == 0) {
            $this->x = 0;
        }
    }

    /**
     * @psalm-assert !null $this->x
     */
    public function assertProperty() : void {
        if (is_null($this->x)) {
            throw new RuntimeException();
        }
    }
}

$a = new A();
$a->maybeAssignX();
$a->assertProperty();
echo (2 * $a->x);
