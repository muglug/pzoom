<?php
interface I {
    public function bat(): void;
}

function takesI(I $i): void {}
function takesA(A $a): void {}
/** @param A&I $a */
function takesAandI($a): void {}
/** @param I&A $a */
function takesIandA($a): void {}

class A {
    /**
     * @return A&I|null
     */
    public function foo() {
        if ($this instanceof I) {
            $this->bar();
            $this->bat();

            takesA($this);
            takesI($this);
            takesAandI($this);
            takesIandA($this);
        }

        return null;
    }

    protected function bar(): void {}
}

class B extends A implements I {
    public function bat(): void {}
}
