<?php
class User {}

/**
 * @template-extends ArrayIterator<array-key, User>
 */
class Users extends ArrayIterator
{
    public function __construct(User ...$users) {
        parent::__construct($users);
    }
}