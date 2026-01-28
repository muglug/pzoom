<?php
class ParentClass
{
    /**
     * @readonly
     * @var null|string
     */
    protected $mightExist;
}

class ChildClass extends ParentClass
{
    /** @var string */
    protected $mightExist = "";
}
