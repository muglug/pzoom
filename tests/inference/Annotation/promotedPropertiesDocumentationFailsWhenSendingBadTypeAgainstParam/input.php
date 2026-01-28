<?php
final class UserRole
{
    /** @psalm-param stdClass $id */
    public function __construct(
        protected $id
    ) {
    }

}
new UserRole("a");
                    
