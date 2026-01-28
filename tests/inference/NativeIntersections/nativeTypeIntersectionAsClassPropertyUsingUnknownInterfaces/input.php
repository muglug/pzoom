<?php
class C {
    private \Example\Unknown\A&\Example\Unknown\B $other;
    public function __construct()
    {
        $this->other = new \Example\Unknown\AB();
    }
}
                
