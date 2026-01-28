<?php
interface A {}
interface B {}
class C implements A, B {}
class D {
    private A&B $intersection;
    public function __construct()
    {
        $this->intersection = new C();
    }
}
                
