<?php
final class UserRole
{
    /** @psalm-param stdClass $id */
    public function __construct(
        protected $id,
        /** @psalm-var stdClass */
        protected $id2
    ) {
    }
}

new UserRole(new stdClass(), new stdClass());
