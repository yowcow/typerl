use strict;
use warnings;
use Test::More;
use Point ();
use Point::Label ();
use Math ();

my $p = Point->new(x => 1, y => 2);
is($p->x, 1, 'Point->new(x => 1, y => 2)->x is 1');
is(ref($p), 'Point', 'blessed into Point');

ok(!eval { Point->new(x => "abc", y => 2); 1 }, 'non-Int x is rejected at entry');
like($@, qr/did not pass type constraint/, 'Type::Tiny reports the violation');
ok(!eval { Point->new(y => 2); 1 }, 'missing x is rejected at entry');
ok(eval { Point->new(x => 1, y => 2, label => undef); 1 }, 'Optional accepts undef (Maybe)');
ok(!eval { Point->new(x => 1, y => 2, label => [1]); 1 }, 'Optional[Str] rejects a non-Str');

{
    package Point::Sub;
    our @ISA = ('Point');
}
my $child = bless { x => 1, y => 2 }, 'Point::Sub';
ok(!eval { $child->x; 1 }, 'subclass instance is rejected by the exact class check (C1)');
like($@, qr/did not pass type constraint/, 'rejected by Type::Tiny');
ok(!eval { Point::x(bless({}, 'Other')); 1 }, 'foreign object is rejected');

{
    package Point::Label::Sub;
    our @ISA = ('Point::Label');
}
my $label = Point::Label->new(text => "t");
ok(eval { Point->new(x => 1, y => 2, tag => $label); 1 }, 'delegated object is accepted');
my $sub_label = bless { text => "t" }, 'Point::Label::Sub';
ok(!eval { Point->new(x => 1, y => 2, tag => $sub_label); 1 }, 'subclass of a field class is rejected inside Maybe (C1)');

is(Math::add(1, 2), 3, 'public function works');
ok(!eval { Math::add("a", 2); 1 }, 'public function checks positional args');

done_testing;
