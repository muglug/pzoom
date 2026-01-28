<?php
class ParentClass
{
    /** @var null|string */
    protected $mightExist;

    protected ?string $mightExistNative = null;

    /** @var string */
    protected $doesExist = "";

    protected string $doesExistNative = "";
}

class ChildClass extends ParentClass
{
    /** @var null|string */
    protected $mightExist = "";

    protected ?string $mightExistNative = null;

    /** @var string */
    protected $doesExist = "";

    protected string $doesExistNative = "";
}
