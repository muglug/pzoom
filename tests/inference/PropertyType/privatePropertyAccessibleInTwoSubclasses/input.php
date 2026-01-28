<?php
class A {
    public function __construct() {}
}
class B extends A {
    /**
     * @var int
     */
    private $prop;

    public function __construct()
    {
        parent::__construct();
        $this->prop = 1;
    }
}
class C extends A {
    /**
     * @var int
     */
    private $prop;

    public function __construct()
    {
        parent::__construct();
        $this->prop = 2;
    }
}
