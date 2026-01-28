<?php
final class UserRole
{
    public function __construct(
        /** @psalm-var stdClass */
        protected $id2
    ) {
    }
}

new UserRole("a");
                    
