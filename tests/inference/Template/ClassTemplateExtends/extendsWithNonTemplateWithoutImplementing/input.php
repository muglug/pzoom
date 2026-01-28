<?php
/**
 * @template T as array-key
 */
abstract class User
{
    /**
     * @var T
     */
    private $id;
    /**
     * @param T $id
     */
    public function __construct($id)
    {
        $this->id = $id;
    }
    /**
     * @return T
     */
    public function getID()
    {
        return $this->id;
    }
}

/**
 * @template-extends User<int>
 */
class AppUser extends User {}

$au = new AppUser(-1);
$id = $au->getId();