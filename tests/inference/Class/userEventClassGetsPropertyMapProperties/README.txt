Psalm applies its PropertyMap (dictionaries/PropertyMap.php) in
ClassLikeNodeScanner::finish() to ANY class whose fully-qualified name
matches a map entry - including a user-defined global `class Event {}`,
which therefore acquires pecl Event's properties like `pending: bool`.
Reading `$e->pending` is thus NOT an UndefinedPropertyFetch (verified
against real Psalm). The mapped properties carry no source location, so
they are exempt from MissingConstructor / PropertyNotSetInConstructor.
