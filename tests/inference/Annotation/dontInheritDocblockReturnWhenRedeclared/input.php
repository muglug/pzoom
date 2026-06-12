<?php
interface Id {}

class UserId implements Id {}

interface Entity {
    /** @psalm-return Id */
    function id(): Id;
}

class User implements Entity {
    public function id(): UserId {
        return new UserId();
    }
}
