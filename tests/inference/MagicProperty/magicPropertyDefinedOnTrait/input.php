<?php
class UserRecord
{
    use UserFields;

    private array $props = [];

    public function __get(string $field)
    {
        return $this->props[$field];
    }

    /**
     * @param mixed $value
     */
    public function __set(string $field, $value) : void
    {
        $this->props[$field] = $value;
    }
}

/**
 * @property mixed $email
 * @property mixed $password
 * @property mixed $last_login_at
 */
trait UserFields {}

$record = new UserRecord();
$record->email;
$record->password;
$record->last_login_at = new DateTimeImmutable("now");
