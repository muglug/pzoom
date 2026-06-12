<?php
declare(strict_types=1);

/** @psalm-immutable */
final class UserList
{
    /**
     * @throws InvalidArgumentException
     */
    public function validate(): void
    {
        // Some validation happens here
        throw new \InvalidArgumentException();
    }
}

$a = new UserList();
$a->validate();

