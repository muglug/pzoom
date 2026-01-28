<?php
namespace Q;

class Base
{
    /**
     * @var string
     */
    private $aString;

    public function __construct()
    {
        $this->aString = "aa";
        echo($this->aString);
    }
}

class Descendant extends Base
{
    /**
     * @var bool
     */
    private $aBool;

    public function __construct()
    {
        parent::__construct();
        $this->aBool = true;
    }
}
