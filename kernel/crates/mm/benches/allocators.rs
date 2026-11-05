#[path = "../tests/support/mod.rs"]
mod support;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use kmm::slab::SlabCache;
use std::time::Duration;
use support::{HostSlabBackend, buddy_allocator};

fn slab_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("slab");
    group.measurement_time(Duration::from_secs(5));

    let mut cache = SlabCache::<64, _>::new(HostSlabBackend::new(16));
    group.throughput(Throughput::Elements(1));
    group.bench_function("allocate deallocate 64b", |bencher| {
        bencher.iter(|| {
            let object = cache.allocate().unwrap();
            black_box(object);
            cache.deallocate(object).unwrap();
        });
    });

    for batch_size in [32usize, 256] {
        let mut cache = SlabCache::<64, _>::new(HostSlabBackend::new(16));
        let mut objects = Vec::with_capacity(batch_size);
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("burst allocate deallocate 64b", batch_size),
            &batch_size,
            |bencher, &batch_size| {
                bencher.iter(|| {
                    for _ in 0..batch_size {
                        objects.push(cache.allocate().unwrap());
                    }
                    for object in objects.drain(..).rev() {
                        cache.deallocate(object).unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

fn buddy_benchmarks(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("buddy");
    group.measurement_time(Duration::from_secs(5));

    let mut allocator = buddy_allocator(10);
    group.throughput(Throughput::Elements(1));
    group.bench_function("split allocate coalesce order 0", |bencher| {
        bencher.iter(|| {
            let page = allocator.alloc(0).unwrap();
            black_box(page);
            allocator.free(page, 0).unwrap();
        });
    });

    for batch_size in [32usize, 256] {
        let mut allocator = buddy_allocator(10);
        let mut allocations = Vec::with_capacity(batch_size);
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("burst allocate deallocate order 0", batch_size),
            &batch_size,
            |bencher, &batch_size| {
                bencher.iter(|| {
                    for _ in 0..batch_size {
                        allocations.push(allocator.alloc(0).unwrap());
                    }
                    for page in allocations.drain(..).rev() {
                        allocator.free(page, 0).unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(allocators, slab_benchmarks, buddy_benchmarks);
criterion_main!(allocators);
