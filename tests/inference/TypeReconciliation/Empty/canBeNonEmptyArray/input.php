<?php
function _processScopes($scopes) : void {
    if (!is_array($scopes) && !empty($scopes)) {
        $scopes = explode(" ", trim($scopes));
    } else {
        // false is allowed here
    }

    if (empty($scopes)){}
}
