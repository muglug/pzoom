<?php
namespace FooBar;

class Datetime extends \DateTime
{
    public static function createFromInterface(\DateTimeInterface $object): static
    {
        return parent::createFromInterface($object);
    }
}
