<?php
interface A {}
interface B {}
class AB implements A, B {}
class C {
    private A&B $other;
    public function __construct()
    {
        $this->other = new AB();
    }
}
                
