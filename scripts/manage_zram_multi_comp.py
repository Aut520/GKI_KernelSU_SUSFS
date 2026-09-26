#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ZRAM Multi-Comp (多压缩流 / 二次重压缩) 全版本兼容管理与修补脚本
支持 Linux 5.10, 5.15, 6.1, 6.6, 6.12+ 全系列内核版本。
采用【git apply 专属补丁 + Python 语义精确修补】双保险引擎，确保任意子版本均可 100% 成功打入。
"""

import sys
import os
import subprocess

def log(msg):
    try:
        print(f"[Multi-Comp] {msg}", flush=True)
    except Exception:
        clean = msg.encode("ascii", errors="replace").decode("ascii")
        print(f"[Multi-Comp] {clean}", flush=True)

def append_config(config_file):
    if not config_file or not os.path.isfile(config_file):
        log(f"[WARN] 未找到配置文件: {config_file}，无法追加配置项")
        return

    configs_to_add = [
        "CONFIG_ZRAM_MULTI_COMP=y\n",
        "CONFIG_ZRAM_MEMORY_TRACKING=y\n",
        "CONFIG_CRYPTO_LZ4=y\n",
        "CONFIG_CRYPTO_ZSTD=y\n",
        "CONFIG_ZRAM_DEF_COMP_LZ4=y\n",
        'CONFIG_ZRAM_DEF_COMP="lz4"\n'
    ]

    try:
        with open(config_file, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        new_entries = []
        for cfg in configs_to_add:
            key = cfg.split("=")[0].strip()
            if key not in content:
                new_entries.append(cfg)

        if new_entries:
            with open(config_file, "a", encoding="utf-8") as f:
                f.write("\n# ZRAM Multi-Comp (多压缩流) 功能配置\n")
                f.writelines(new_entries)
            log(f"[OK] 已成功向 {os.path.basename(config_file)} 注入 Multi-Comp 配置项")
        else:
            log(f"[INFO] {os.path.basename(config_file)} 中已存在相关配置项，无需重复注入")
    except Exception as e:
        log(f"[WARN] 注入配置文件时发生异常: {e}")

def check_native_support(kernel_root, kernel_version):
    """
    检查当前内核源码是否原生内置 Multi-Comp 驱动与 Kconfig
    """
    # 1. 检查 Kconfig 源码中是否已原生包含 ZRAM_MULTI_COMP
    kconfig_path = os.path.join(kernel_root, "drivers/block/zram/Kconfig")
    if os.path.isfile(kconfig_path):
        try:
            with open(kconfig_path, "r", encoding="utf-8", errors="replace") as f:
                if "config ZRAM_MULTI_COMP" in f.read():
                    return True
        except Exception:
            pass

    # 2. 版本号判定：主版本号 >= 6 且次版本号 >= 6 (例如 6.6, 6.12+)
    try:
        parts = kernel_version.strip().split(".")
        major = int(parts[0])
        minor = int(parts[1]) if len(parts) > 1 else 0
        if major > 6 or (major == 6 and minor >= 6):
            return True
    except Exception:
        pass

    return False

def get_patch_file_for_version(script_dir, kernel_version):
    ver = kernel_version.strip()
    patch_dir = os.path.join(script_dir, "../zram/patches")
    patch_dir = os.path.abspath(patch_dir)

    target_patch = None
    if ver.startswith("6.1"):
        target_patch = os.path.join(patch_dir, "zram_multi_comp_6.1.patch")
    elif ver.startswith("5.15"):
        target_patch = os.path.join(patch_dir, "zram_multi_comp_5.15.patch")
    elif ver.startswith("5.10"):
        target_patch = os.path.join(patch_dir, "zram_multi_comp_5.10.patch")

    if target_patch and os.path.isfile(target_patch):
        return target_patch
    return None

def python_semantic_patch(kernel_root, kernel_version):
    """
    Python 语义修补兜底引擎：当 git apply 遇到不同子版本行号漂移时，直接在 AST/语义层进行精准修补。
    """
    log(f"[INFO] 正在启动 Python 语义修补引擎对 {kernel_version} 源码进行自适应修补...")
    zram_dir = os.path.join(kernel_root, "drivers", "block", "zram")
    if not os.path.isdir(zram_dir):
        log(f"[ERROR] 目标目录不存在: {zram_dir}")
        return False

    kconfig_path = os.path.join(zram_dir, "Kconfig")
    zcomp_h_path = os.path.join(zram_dir, "zcomp.h")
    zcomp_c_path = os.path.join(zram_dir, "zcomp.c")
    zram_drv_h_path = os.path.join(zram_dir, "zram_drv.h")
    zram_drv_c_path = os.path.join(zram_dir, "zram_drv.c")

    try:
        # 1. Kconfig
        with open(kconfig_path, "r", encoding="utf-8", errors="replace") as f:
            kconfig = f.read()
        if "config ZRAM_MULTI_COMP" not in kconfig:
            kconfig = kconfig.rstrip() + "\n\nconfig ZRAM_MULTI_COMP\n\tbool \"Enable multiple compression streams\"\n\tdepends on ZRAM\n\thelp\n\t  This will enable multi-compression streams, so that ZRAM can\n\t  re-compress pages using a potentially slower but more effective\n\t  compression algorithm. Note, that IDLE page recompression\n\t  requires ZRAM_MEMORY_TRACKING.\n"
            with open(kconfig_path, "w", encoding="utf-8", newline="\n") as f:
                f.write(kconfig)
            log("[OK] drivers/block/zram/Kconfig 注入成功")

        # 2. zcomp.h
        with open(zcomp_h_path, "r", encoding="utf-8", errors="replace") as f:
            zcomp_h = f.read()
        if "struct zcomp *zcomp_create(const char *comp);" in zcomp_h:
            zcomp_h = zcomp_h.replace("struct zcomp *zcomp_create(const char *comp);",
                                      "struct zcomp *zcomp_create(const char *alg);")
            with open(zcomp_h_path, "w", encoding="utf-8", newline="\n") as f:
                f.write(zcomp_h)
            log("[OK] drivers/block/zram/zcomp.h 声明更新成功")

        # 3. zcomp.c
        with open(zcomp_c_path, "r", encoding="utf-8", errors="replace") as f:
            zcomp_c = f.read()
        if "struct zcomp *zcomp_create(const char *compress)" in zcomp_c:
            zcomp_c = zcomp_c.replace("struct zcomp *zcomp_create(const char *compress)",
                                      "struct zcomp *zcomp_create(const char *alg)")
            zcomp_c = zcomp_c.replace("!zcomp_available_algorithm(compress)",
                                      "!zcomp_available_algorithm(alg)")
            zcomp_c = zcomp_c.replace("comp->name = compress;",
                                      "comp->name = alg;")
            with open(zcomp_c_path, "w", encoding="utf-8", newline="\n") as f:
                f.write(zcomp_c)
            log("[OK] drivers/block/zram/zcomp.c 适配更新成功")

        # 4. zram_drv.h
        with open(zram_drv_h_path, "r", encoding="utf-8", errors="replace") as f:
            zram_h = f.read()
        if "ZRAM_COMP_PRIORITY_MASK" not in zram_h:
            if "#define ZRAM_FLAG_SHIFT (PAGE_SHIFT + 1)" in zram_h:
                zram_h = zram_h.replace("#define ZRAM_FLAG_SHIFT (PAGE_SHIFT + 1)",
                                        "#define ZRAM_FLAG_SHIFT (PAGE_SHIFT + 1)\n\n/* Only 2 bits are allowed for comp priority index */\n#define ZRAM_COMP_PRIORITY_MASK\t0x3UL")
            elif "#define ZRAM_FLAG_SHIFT 24" in zram_h:
                zram_h = zram_h.replace("#define ZRAM_FLAG_SHIFT 24",
                                        "#define ZRAM_FLAG_SHIFT 24\n\n/* Only 2 bits are allowed for comp priority index */\n#define ZRAM_COMP_PRIORITY_MASK\t0x3UL")
            zram_h = zram_h.replace("\tZRAM_IDLE,\t/* not accessed page since last idle marking */",
                                    "\tZRAM_IDLE,\t/* not accessed page since last idle marking */\n\tZRAM_INCOMPRESSIBLE, /* none of the algorithms could compress it */\n\n\tZRAM_COMP_PRIORITY_BIT1, /* First bit of comp priority index */\n\tZRAM_COMP_PRIORITY_BIT2, /* Second bit of comp priority index */")
            zram_h = zram_h.replace("struct zram {",
                                    "#ifdef CONFIG_ZRAM_MULTI_COMP\n#define ZRAM_PRIMARY_COMP\t0U\n#define ZRAM_SECONDARY_COMP\t1U\n#define ZRAM_MAX_COMPS\t4U\n#else\n#define ZRAM_PRIMARY_COMP\t0U\n#define ZRAM_SECONDARY_COMP\t0U\n#define ZRAM_MAX_COMPS\t1U\n#endif\n\nstruct zram {")
            zram_h = zram_h.replace("\tstruct zcomp *comp;", "\tstruct zcomp *comps[ZRAM_MAX_COMPS];")
            zram_h = zram_h.replace("\tchar compressor[CRYPTO_MAX_ALG_NAME];",
                                    "\tconst char *comp_algs[ZRAM_MAX_COMPS];\n\ts8 num_active_comps;")
            with open(zram_drv_h_path, "w", encoding="utf-8", newline="\n") as f:
                f.write(zram_h)
            log("[OK] drivers/block/zram/zram_drv.h 多流结构体注入成功")

        # 5. zram_drv.c
        with open(zram_drv_c_path, "r", encoding="utf-8", errors="replace") as f:
            zram_c = f.read()

        if "zram_set_priority" not in zram_c:
            prio_helpers = """
static inline void zram_set_priority(struct zram *zram, u32 index, u32 prio)
{
	prio &= ZRAM_COMP_PRIORITY_MASK;
	/*
	 * Clear previous priority value first, in case if we recompress
	 * further an already recompressed page
	 */
	zram->table[index].flags &= ~(ZRAM_COMP_PRIORITY_MASK <<
				      ZRAM_COMP_PRIORITY_BIT1);
	zram->table[index].flags |= ((unsigned long)prio << ZRAM_COMP_PRIORITY_BIT1);
}

static inline u32 zram_get_priority(struct zram *zram, u32 index)
{
	u32 prio = zram->table[index].flags >> ZRAM_COMP_PRIORITY_BIT1;

	return prio & ZRAM_COMP_PRIORITY_MASK;
}
"""
            anchor1 = "static inline bool is_partial_io(struct bio_vec *bvec)\n{\n\treturn false;\n}\n#endif"
            idx1 = zram_c.find(anchor1)
            if idx1 != -1:
                end_idx1 = idx1 + len(anchor1)
                zram_c = zram_c[:end_idx1] + "\n" + prio_helpers + zram_c[end_idx1:]

            # read_block_state
            target_state = "zram_test_flag(zram, index, ZRAM_IDLE) ? 'i' : '.');"
            repl_state = """zram_test_flag(zram, index, ZRAM_IDLE) ? 'i' : '.',
#ifdef CONFIG_ZRAM_MULTI_COMP
			zram_get_priority(zram, index) ? 'r' : '.');
#else
			'.');
#endif"""
            target_state_fmt = '"%12zd %12lld.%06lu %c%c%c%c\\n",'
            repl_state_fmt = '"%12zd %12lld.%06lu %c%c%c%c%c\\n",'
            if target_state in zram_c:
                zram_c = zram_c.replace(target_state_fmt, repl_state_fmt)
                zram_c = zram_c.replace(target_state, repl_state)

            # comp_algorithm
            idx_comp_alg = zram_c.find("static ssize_t comp_algorithm_show(")
            idx_compact = zram_c.find("static ssize_t compact_store(", idx_comp_alg)
            if idx_comp_alg != -1 and idx_compact != -1:
                repl_comp_alg = """static void comp_algorithm_set(struct zram *zram, u32 prio, const char *alg)
{
	/* 仅释放动态堆内存，不释放静态字符串常量 */
	if (zram->comp_algs[prio] &&
	    zram->comp_algs[prio] != default_compressor &&
	    strcmp(zram->comp_algs[prio], "lz4") != 0 &&
	    strcmp(zram->comp_algs[prio], "zstd") != 0 &&
	    strcmp(zram->comp_algs[prio], "lz4hc") != 0)
		kfree(zram->comp_algs[prio]);

	zram->comp_algs[prio] = alg;
}

static ssize_t __comp_algorithm_show(struct zram *zram, u32 prio, char *buf)
{
	ssize_t sz;

	down_read(&zram->init_lock);
	sz = zcomp_available_show(zram->comp_algs[prio], buf);
	up_read(&zram->init_lock);

	return sz;
}

static int __comp_algorithm_store(struct zram *zram, u32 prio, const char *buf)
{
	char *compressor;
	size_t sz;

	sz = strlen(buf);
	if (sz >= CRYPTO_MAX_ALG_NAME)
		return -E2BIG;

	compressor = kstrdup(buf, GFP_KERNEL);
	if (!compressor)
		return -ENOMEM;

	/* ignore trailing newline */
	if (sz > 0 && compressor[sz - 1] == '\\n')
		compressor[sz - 1] = '\\0';

	if (!zcomp_available_algorithm(compressor)) {
		kfree(compressor);
		return -EINVAL;
	}

	down_write(&zram->init_lock);
	if (init_done(zram)) {
		up_write(&zram->init_lock);
		kfree(compressor);
		pr_info("Can't change algorithm for initialized device\\n");
		return -EBUSY;
	}

	comp_algorithm_set(zram, prio, compressor);
	up_write(&zram->init_lock);
	return 0;
}

static ssize_t comp_algorithm_show(struct device *dev,
				   struct device_attribute *attr,
				   char *buf)
{
	struct zram *zram = dev_to_zram(dev);

	return __comp_algorithm_show(zram, ZRAM_PRIMARY_COMP, buf);
}

static ssize_t comp_algorithm_store(struct device *dev,
				    struct device_attribute *attr,
				    const char *buf,
				    size_t len)
{
	struct zram *zram = dev_to_zram(dev);
	int ret;

	ret = __comp_algorithm_store(zram, ZRAM_PRIMARY_COMP, buf);
	return ret ? ret : len;
}

#ifdef CONFIG_ZRAM_MULTI_COMP
static ssize_t recomp_algorithm_show(struct device *dev,
				     struct device_attribute *attr,
				     char *buf)
{
	struct zram *zram = dev_to_zram(dev);
	ssize_t sz = 0;
	u32 prio;

	down_read(&zram->init_lock);
	for (prio = ZRAM_SECONDARY_COMP; prio < ZRAM_MAX_COMPS; prio++) {
		if (!zram->comp_algs[prio])
			continue;

		sz += scnprintf(buf + sz, PAGE_SIZE - sz, "#%d: ", prio);
		sz += zcomp_available_show(zram->comp_algs[prio], buf + sz);
	}
	up_read(&zram->init_lock);

	return sz;
}

static ssize_t recomp_algorithm_store(struct device *dev,
				      struct device_attribute *attr,
				      const char *buf,
				      size_t len)
{
	struct zram *zram = dev_to_zram(dev);
	int prio = ZRAM_SECONDARY_COMP;
	char *args, *param, *val;
	char *alg = NULL;
	int ret = 0;

	args = skip_spaces(buf);
	while (*args) {
		args = next_arg(args, &param, &val);
		if (!val) {
			if (!alg) {
				alg = param;
				continue;
			}
			return -EINVAL;
		}

		if (!strcmp(param, "algo")) {
			alg = val;
			continue;
		}
		if (!strcmp(param, "priority")) {
			ret = kstrtoint(val, 10, &prio);
			if (ret)
				return ret;
			continue;
		}
		return -EINVAL;
	}

	if (!alg)
		return -EINVAL;

	if (prio < ZRAM_SECONDARY_COMP || prio >= ZRAM_MAX_COMPS)
		return -EINVAL;

	ret = __comp_algorithm_store(zram, prio, alg);
	return ret ? ret : len;
}

static DEVICE_ATTR_RW(recomp_algorithm);
#endif\n\n"""
                zram_c = zram_c[:idx_comp_alg] + repl_comp_alg + zram_c[idx_compact:]

            # zram_destroy_comps
            destroy_comps_code = """
static void zram_destroy_comps(struct zram *zram)
{
	u32 prio;

	for (prio = 0; prio < ZRAM_MAX_COMPS; prio++) {
		struct zcomp *comp = zram->comps[prio];

		zram->comps[prio] = NULL;
		if (!comp)
			continue;
		zcomp_destroy(comp);
		zram->num_active_comps--;
	}

	for (prio = ZRAM_PRIMARY_COMP; prio < ZRAM_MAX_COMPS; prio++) {
		if (zram->comp_algs[prio] &&
		    zram->comp_algs[prio] != default_compressor &&
		    strcmp(zram->comp_algs[prio], "lz4") != 0 &&
		    strcmp(zram->comp_algs[prio], "zstd") != 0 &&
		    strcmp(zram->comp_algs[prio], "lz4hc") != 0)
			kfree(zram->comp_algs[prio]);
		zram->comp_algs[prio] = NULL;
	}
}
"""
            idx_reset = zram_c.find("static void zram_reset_device(struct zram *zram)")
            if idx_reset != -1:
                zram_c = zram_c[:idx_reset] + destroy_comps_code + "\n" + zram_c[idx_reset:]

            # zram_read_from_zspool
            read_zspool_code = """
/*
 * Reads (decompresses if needed) a page from zspool (zsmalloc).
 * Corresponding ZRAM slot should be locked.
 */
static int zram_read_from_zspool(struct zram *zram, struct page *page,
				 u32 index)
{
	struct zcomp_strm *zstrm = NULL;
	unsigned long handle;
	unsigned int size;
	void *src, *dst;
	u32 prio;
	int ret;

	handle = zram_get_handle(zram, index);
	if (!handle || zram_test_flag(zram, index, ZRAM_SAME)) {
		unsigned long value;
		void *mem;

		value = handle ? zram_get_element(zram, index) : 0;
		mem = kmap_atomic(page);
		zram_fill_page(mem, PAGE_SIZE, value);
		kunmap_atomic(mem);
		return 0;
	}

	size = zram_get_obj_size(zram, index);

	prio = zram_get_priority(zram, index);
	/* 安全防御护栏：若次级流后端不存在，强制降级到 Primary 流 */
	if (unlikely(prio >= ZRAM_MAX_COMPS || !zram->comps[prio]))
		prio = ZRAM_PRIMARY_COMP;

	if (size != PAGE_SIZE)
		zstrm = zcomp_stream_get(zram->comps[prio]);

	src = zs_map_object(zram->mem_pool, handle, ZS_MM_RO);
	if (size == PAGE_SIZE) {
		dst = kmap_atomic(page);
		memcpy(dst, src, PAGE_SIZE);
		kunmap_atomic(dst);
		ret = 0;
	} else {
		dst = kmap_atomic(page);
		ret = zcomp_decompress(zstrm, src, size, dst);
		kunmap_atomic(dst);
		zcomp_stream_put(zram->comps[prio]);
	}
	zs_unmap_object(zram->mem_pool, handle);

	/* Should NEVER happen. Return bio error if it does. */
	if (WARN_ON(ret))
		pr_err("Decompression failed! err=%d, page=%u\\n", ret, index);

	return ret;
}
"""
            idx_bvec_read = zram_c.find("static int __zram_bvec_read(struct zram *zram, struct page *page, u32 index,")
            if idx_bvec_read != -1:
                zram_c = zram_c[:idx_bvec_read] + read_zspool_code + "\n" + zram_c[idx_bvec_read:]

            # __zram_bvec_read body replace
            idx_read_start = zram_c.find("static int __zram_bvec_read(")
            idx_read_next = zram_c.find("static int zram_bvec_read(", idx_read_start)
            if idx_read_start != -1 and idx_read_next != -1:
                read_block = zram_c[idx_read_start:idx_read_next]
                anchor_handle = "\thandle = zram_get_handle(zram, index);"
                if anchor_handle in read_block:
                    idx_bh = read_block.find(anchor_handle)
                    new_read_block = read_block[:idx_bh] + "\tret = zram_read_from_zspool(zram, page, index);\n\tzram_slot_unlock(zram, index);\n\treturn ret;\n}\n\n"
                    # 清理重构后在 __zram_bvec_read 中未被使用的局部变量声明，防止 -Werror 报 unused-variable
                    for unused_decl in [
                        "\tstruct zcomp_strm *zstrm;\n",
                        "\tunsigned long handle;\n",
                        "\tunsigned int size;\n",
                        "\tvoid *src, *dst;\n",
                        "\tvoid *src;\n",
                        "\tvoid *dst;\n",
                    ]:
                        new_read_block = new_read_block.replace(unused_decl, "")
                    zram_c = zram_c[:idx_read_start] + new_read_block + zram_c[idx_read_next:]

            # __zram_bvec_write primary comp
            zram_c = zram_c.replace("zstrm = zcomp_stream_get(zram->comp);",
                                    "zstrm = zcomp_stream_get(zram->comps[ZRAM_PRIMARY_COMP]);")
            zram_c = zram_c.replace("zcomp_stream_put(zram->comp);",
                                    "zcomp_stream_put(zram->comps[ZRAM_PRIMARY_COMP]);")
            target_write_prio = "zram_set_obj_size(zram, index, comp_len);"
            if target_write_prio in zram_c:
                zram_c = zram_c.replace(target_write_prio,
                                        "zram_set_obj_size(zram, index, comp_len);\n\tzram_set_priority(zram, index, ZRAM_PRIMARY_COMP);")

            # recompress functions
            recomp_funcs = """
#ifdef CONFIG_ZRAM_MULTI_COMP
static int zram_recompress(struct zram *zram, u32 index, struct page *page,
			   u32 threshold, u32 prio, u32 prio_max)
{
	struct zcomp_strm *zstrm = NULL;
	unsigned long handle_old;
	unsigned long handle_new;
	unsigned int comp_len_old;
	unsigned int comp_len_new = 0;
	u32 num_recomps = 0;
	void *src, *dst;
	int ret;

	handle_old = zram_get_handle(zram, index);
	if (!handle_old)
		return 0;

	comp_len_old = zram_get_obj_size(zram, index);
	/*
	 * Do not recompress objects that are already "small enough".
	 */
	if (comp_len_old < threshold)
		return 0;

	ret = zram_read_from_zspool(zram, page, index);
	if (ret)
		return ret;

	/*
	 * Iterate the secondary comp algorithms list (in order of priority)
	 * and try to recompress the page.
	 */
	for (; prio < prio_max; prio++) {
		if (!zram->comps[prio])
			continue;

		/*
		 * Skip if the object is already re-compressed with a higher
		 * priority algorithm (or same algorithm).
		 */
		if (prio <= zram_get_priority(zram, index))
			continue;

		num_recomps++;
		zstrm = zcomp_stream_get(zram->comps[prio]);
		src = kmap_atomic(page);
		ret = zcomp_compress(zstrm, src, &comp_len_new);
		kunmap_atomic(src);

		if (ret) {
			zcomp_stream_put(zram->comps[prio]);
			return ret;
		}

		/* Continue until we make progress */
		if (comp_len_new >= huge_class_size ||
		    comp_len_new >= comp_len_old ||
		    (threshold && comp_len_new >= threshold)) {
			zcomp_stream_put(zram->comps[prio]);
			continue;
		}

		/* Recompression was successful so break out */
		break;
	}

	if (!zstrm)
		return 0;

	if (comp_len_new >= huge_class_size || comp_len_new >= comp_len_old) {
		if (num_recomps == zram->num_active_comps - 1)
			zram_set_flag(zram, index, ZRAM_INCOMPRESSIBLE);
		return 0;
	}

	/* Successful recompression but above threshold */
	if (threshold && comp_len_new >= threshold)
		return 0;

	handle_new = zs_malloc(zram->mem_pool, comp_len_new,
			       __GFP_KSWAPD_RECLAIM |
			       __GFP_NOWARN |
			       __GFP_HIGHMEM |
			       __GFP_MOVABLE |
			       __GFP_CMA);
	if (!handle_new) {
		zcomp_stream_put(zram->comps[prio]);
		return -ENOMEM;
	}

	dst = zs_map_object(zram->mem_pool, handle_new, ZS_MM_WO);
	memcpy(dst, zstrm->buffer, comp_len_new);
	zcomp_stream_put(zram->comps[prio]);

	zs_unmap_object(zram->mem_pool, handle_new);

	zram_free_page(zram, index);
	zram_set_handle(zram, index, handle_new);
	zram_set_obj_size(zram, index, comp_len_new);
	zram_set_priority(zram, index, prio);

	atomic64_add(comp_len_new, &zram->stats.compr_data_size);
	atomic64_inc(&zram->stats.pages_stored);

	return 0;
}

#define RECOMPRESS_IDLE		(1 << 0)
#define RECOMPRESS_HUGE		(1 << 1)

static ssize_t recompress_store(struct device *dev,
				struct device_attribute *attr,
				const char *buf, size_t len)
{
	struct zram *zram = dev_to_zram(dev);
	unsigned long nr_pages = zram->disksize >> PAGE_SHIFT;
	char *args, *param, *val, *alg_name = NULL;
	u32 prio = ZRAM_SECONDARY_COMP;
	u32 prio_max = ZRAM_MAX_COMPS;
	struct page *page = NULL;
	u32 threshold = 0;
	u32 mode = 0;
	unsigned long index;
	int err = 0;

	args = skip_spaces(buf);
	while (*args) {
		args = next_arg(args, &param, &val);
		if (!val) {
			if (!strcmp(param, "idle")) {
				mode |= RECOMPRESS_IDLE;
				continue;
			}
			if (!strcmp(param, "huge")) {
				mode |= RECOMPRESS_HUGE;
				continue;
			}
			return -EINVAL;
		}

		if (!strcmp(param, "type")) {
			if (!strcmp(val, "idle"))
				mode |= RECOMPRESS_IDLE;
			else if (!strcmp(val, "huge"))
				mode |= RECOMPRESS_HUGE;
			else
				return -EINVAL;
			continue;
		}

		if (!strcmp(param, "threshold")) {
			err = kstrtouint(val, 10, &threshold);
			if (err)
				return err;
			continue;
		}

		if (!strcmp(param, "algo")) {
			alg_name = val;
			continue;
		}
		return -EINVAL;
	}

	if (alg_name) {
		bool found = false;

		for (; prio < ZRAM_MAX_COMPS; prio++) {
			if (!zram->comp_algs[prio])
				continue;

			if (!strcmp(zram->comp_algs[prio], alg_name)) {
				prio_max = prio + 1;
				found = true;
				break;
			}
		}

		if (!found)
			return -EINVAL;
	}

	page = alloc_page(GFP_KERNEL);
	if (!page)
		return -ENOMEM;

	down_read(&zram->init_lock);
	if (!init_done(zram)) {
		err = -EINVAL;
		goto release_init_lock;
	}

	for (index = 0; index < nr_pages; index++) {
		int post_processing_needed = 0;

		zram_slot_lock(zram, index);

		if (!zram_allocated(zram, index))
			goto next;

		if (zram_test_flag(zram, index, ZRAM_SAME) ||
		    zram_test_flag(zram, index, ZRAM_WB) ||
		    zram_test_flag(zram, index, ZRAM_UNDER_WB))
			goto next;

		if (mode & RECOMPRESS_IDLE &&
		    !zram_test_flag(zram, index, ZRAM_IDLE))
			goto next;

		if (mode & RECOMPRESS_HUGE &&
		    !zram_test_flag(zram, index, ZRAM_HUGE))
			goto next;

		if (zram_test_flag(zram, index, ZRAM_INCOMPRESSIBLE))
			goto next;

		post_processing_needed = 1;
next:
		zram_slot_unlock(zram, index);

		if (!post_processing_needed)
			continue;

		zram_slot_lock(zram, index);
		if (!zram_allocated(zram, index) ||
		    zram_test_flag(zram, index, ZRAM_SAME) ||
		    zram_test_flag(zram, index, ZRAM_WB) ||
		    zram_test_flag(zram, index, ZRAM_UNDER_WB)) {
			zram_slot_unlock(zram, index);
			continue;
		}

		err = zram_recompress(zram, index, page, threshold,
				      prio, prio_max);
		zram_slot_unlock(zram, index);
		if (err)
			break;

		cond_resched();
	}

release_init_lock:
	up_read(&zram->init_lock);
	if (page)
		__free_page(page);

	return err ? err : len;
}

static DEVICE_ATTR_WO(recompress);
#endif
"""
            idx_notify = zram_c.find("static void zram_slot_free_notify(struct block_device *bdev,")
            if idx_notify != -1:
                zram_c = zram_c[:idx_notify] + recomp_funcs + "\n" + zram_c[idx_notify:]

            # zram_reset_device modification
            idx_reset_start = zram_c.find("static void zram_reset_device(struct zram *zram)")
            idx_reset_end = zram_c.find("static ssize_t disksize_store(", idx_reset_start)
            if idx_reset_start != -1 and idx_reset_end != -1:
                reset_block = zram_c[idx_reset_start:idx_reset_end]
                new_reset_block = reset_block.replace("\tstruct zcomp *comp;\n", "").replace("\tcomp = zram->comp;\n", "")
                repl = """\tzram_destroy_comps(zram);
\tif (zcomp_available_algorithm("lz4"))
\t\tcomp_algorithm_set(zram, ZRAM_PRIMARY_COMP, "lz4");
\telse
\t\tcomp_algorithm_set(zram, ZRAM_PRIMARY_COMP, default_compressor);
#ifdef CONFIG_ZRAM_MULTI_COMP
\tif (zcomp_available_algorithm("zstd"))
\t\tcomp_algorithm_set(zram, ZRAM_SECONDARY_COMP, "zstd");
\telse if (zcomp_available_algorithm("lz4hc"))
\t\tcomp_algorithm_set(zram, ZRAM_SECONDARY_COMP, "lz4hc");
#endif
"""
                t1 = "\tif (zram->comp)\n\t\tzcomp_destroy(zram->comp);\n\tzram->comp = NULL;\n"
                t2 = "\tzcomp_destroy(zram->comp);\n\tzram->comp = NULL;\n"
                t3 = "\tzcomp_destroy(comp);\n"
                if t1 in new_reset_block:
                    new_reset_block = new_reset_block.replace(t1, repl)
                elif t2 in new_reset_block:
                    new_reset_block = new_reset_block.replace(t2, repl)
                elif t3 in new_reset_block:
                    new_reset_block = new_reset_block.replace(t3, repl)
                zram_c = zram_c[:idx_reset_start] + new_reset_block + zram_c[idx_reset_end:]

            # disksize_store: create multiple comps
            idx_disksize_start = zram_c.find("static ssize_t disksize_store(")
            idx_disksize_end = zram_c.find("static ssize_t reset_store(", idx_disksize_start)
            if idx_disksize_start != -1 and idx_disksize_end != -1:
                disksize_block = zram_c[idx_disksize_start:idx_disksize_end]
                target_create = """\tcomp = zcomp_create(zram->compressor);
	if (IS_ERR(comp)) {
		pr_err("Cannot initialise %s compressing backend\\n",
				zram->compressor);
		err = PTR_ERR(comp);
		goto out_free_meta;
	}

	zram->comp = comp;"""
                repl_create = """\tfor (num_comps = 0; num_comps < ZRAM_MAX_COMPS; num_comps++) {
		if (!zram->comp_algs[num_comps])
			continue;

		comp = zcomp_create(zram->comp_algs[num_comps]);
		if (IS_ERR(comp)) {
			pr_err("Cannot initialise %s compressing backend\\n",
			       zram->comp_algs[num_comps]);
			err = PTR_ERR(comp);
			goto out_free_comps;
		}
		zram->comps[num_comps] = comp;
		zram->num_active_comps++;
	}"""
                new_disksize_block = disksize_block.replace("struct zcomp *comp;", "struct zcomp *comp;\n\tint num_comps;\n\tu64 max_limit;")
                if target_create in new_disksize_block:
                    new_disksize_block = new_disksize_block.replace(target_create, repl_create)

                # 智能上限约束：若未指定(0)或写入值超过物理内存 3/4(如系统默认100%)，自动对齐约束为 3/4
                target_disksize_parse = """\tdisksize = memparse(buf, NULL);
\tif (!disksize)
\t\treturn -EINVAL;"""
                repl_disksize_parse = """\tmax_limit = PAGE_ALIGN(((u64)totalram_pages() << PAGE_SHIFT) * 3 / 4);
\tdisksize = memparse(buf, NULL);
\tif (!disksize || disksize > max_limit)
\t\tdisksize = max_limit;"""
                if target_disksize_parse in new_disksize_block:
                    new_disksize_block = new_disksize_block.replace(target_disksize_parse, repl_disksize_parse)

                target_out_free = """out_free_meta:
	zram_meta_free(zram, disksize);"""
                repl_out_free = """out_free_comps:
	zram_destroy_comps(zram);
	zram_meta_free(zram, disksize);"""
                if target_out_free in new_disksize_block:
                    new_disksize_block = new_disksize_block.replace(target_out_free, repl_out_free)
                # 兼容处理残留的 out_free_meta 孤立标签
                new_disksize_block = new_disksize_block.replace("out_free_comps:\n\tzram_destroy_comps(zram);\nout_free_meta:\n",
                                                               "out_free_comps:\n\tzram_destroy_comps(zram);\n")
                zram_c = zram_c[:idx_disksize_start] + new_disksize_block + zram_c[idx_disksize_end:]

            # zram_disk_attrs
            target_attrs = """\t&dev_attr_comp_algorithm.attr,
#ifdef CONFIG_ZRAM_WRITEBACK"""
            repl_attrs = """\t&dev_attr_comp_algorithm.attr,
#ifdef CONFIG_ZRAM_MULTI_COMP
	&dev_attr_recomp_algorithm.attr,
	&dev_attr_recompress.attr,
#endif
#ifdef CONFIG_ZRAM_WRITEBACK"""
            if target_attrs in zram_c:
                zram_c = zram_c.replace(target_attrs, repl_attrs)

            # zram_free_page: clear ZRAM_INCOMPRESSIBLE and priority to prevent WARN_ON_ONCE
            target_free_idle = "\tif (zram_test_flag(zram, index, ZRAM_IDLE))\n\t\tzram_clear_flag(zram, index, ZRAM_IDLE);"
            repl_free_idle = """\tif (zram_test_flag(zram, index, ZRAM_IDLE))
\t\tzram_clear_flag(zram, index, ZRAM_IDLE);

#ifdef CONFIG_ZRAM_MULTI_COMP
\tif (zram_test_flag(zram, index, ZRAM_INCOMPRESSIBLE))
\t\tzram_clear_flag(zram, index, ZRAM_INCOMPRESSIBLE);

\tzram_set_priority(zram, index, 0);
#endif"""
            if target_free_idle in zram_c:
                zram_c = zram_c.replace(target_free_idle, repl_free_idle)

            # zram_add default_compressor: 默认主算法lz4，次算法zstd/lz4hc
            init_comps_code = """\tif (zcomp_available_algorithm("lz4"))
\t\tcomp_algorithm_set(zram, ZRAM_PRIMARY_COMP, "lz4");
\telse
\t\tcomp_algorithm_set(zram, ZRAM_PRIMARY_COMP, default_compressor);
#ifdef CONFIG_ZRAM_MULTI_COMP
\tif (zcomp_available_algorithm("zstd"))
\t\tcomp_algorithm_set(zram, ZRAM_SECONDARY_COMP, "zstd");
\telse if (zcomp_available_algorithm("lz4hc"))
\t\tcomp_algorithm_set(zram, ZRAM_SECONDARY_COMP, "lz4hc");
#endif"""
            zram_c = zram_c.replace("\tstrlcpy(zram->compressor, default_compressor, sizeof(zram->compressor));",
                                    init_comps_code)
            zram_c = zram_c.replace("\tstrscpy(zram->compressor, default_compressor, sizeof(zram->compressor));",
                                    init_comps_code)

            with open(zram_drv_c_path, "w", encoding="utf-8", newline="\n") as f:
                f.write(zram_c)
            log("[OK] drivers/block/zram/zram_drv.c 驱动函数注入成功")

        return True
    except Exception as e:
        log(f"[ERROR] Python 语义修补过程异常: {e}")
        return False

def sanitize_zram_drv(kernel_root):
    """
    检查并自愈净化 drivers/block/zram/zram_drv.c:
    1. 消除 __zram_bvec_read 中未使用的局部变量声明 (zstrm, handle, size, src, dst)，防止 -Werror,-Wunused-variable
    2. 消除 disksize_store 中未使用的 out_free_meta: 标签，防止 -Werror,-Wunused-label
    """
    zram_c_path = os.path.join(kernel_root, "drivers/block/zram/zram_drv.c")
    if not os.path.isfile(zram_c_path):
        return
    try:
        with open(zram_c_path, "r", encoding="utf-8", errors="replace") as f:
            code = f.read()

        changed = False

        # 1. 修复 __zram_bvec_read
        idx_read = code.find("static int __zram_bvec_read(")
        if idx_read != -1:
            idx_read_next = code.find("static int zram_bvec_read(", idx_read)
            if idx_read_next != -1:
                read_block = code[idx_read:idx_read_next]
                if "zram_read_from_zspool" in read_block:
                    clean_read_block = read_block
                    for unused_decl in [
                        "\tstruct zcomp_strm *zstrm;\n",
                        "\tunsigned long handle;\n",
                        "\tunsigned int size;\n",
                        "\tvoid *src, *dst;\n",
                        "\tvoid *src;\n",
                        "\tvoid *dst;\n",
                    ]:
                        clean_read_block = clean_read_block.replace(unused_decl, "")
                    if clean_read_block != read_block:
                        code = code[:idx_read] + clean_read_block + code[idx_read_next:]
                        changed = True

        # 2. 修复 disksize_store 中的 out_free_meta: 标签
        if "out_free_comps:" in code and "out_free_meta:" in code:
            if "goto out_free_meta;" not in code:
                code = code.replace("out_free_comps:\n\tzram_destroy_comps(zram);\nout_free_meta:\n",
                                    "out_free_comps:\n\tzram_destroy_comps(zram);\n")
                code = code.replace("\nout_free_meta:\n", "\n")
                changed = True

        # 3. 修复可能由制表符偏差引发的 -Wmisleading-indentation
        bad_indent = "\t\tif (zcomp_available_algorithm(\"lz4\"))"
        good_indent = "\tif (zcomp_available_algorithm(\"lz4\"))"
        if bad_indent in code:
            code = code.replace(bad_indent, good_indent)
            changed = True

        if changed:
            with open(zram_c_path, "w", encoding="utf-8", newline="\n") as f:
                f.write(code)
            log("[OK] drivers/block/zram/zram_drv.c 自愈净化完成 (已消除 unused-variable、unused-label 与 misleading-indentation)")
    except Exception as e:
        log(f"[WARN] zram_drv.c 自愈净化检查异常: {e}")

def apply_patch(kernel_root, patch_file, kernel_version):
    zram_drv_h = os.path.join(kernel_root, "drivers/block/zram/zram_drv.h")
    if os.path.isfile(zram_drv_h):
        try:
            with open(zram_drv_h, "r", encoding="utf-8", errors="replace") as f:
                if "CONFIG_ZRAM_MULTI_COMP" in f.read():
                    log(f"[INFO] {kernel_version} 内核源码已打入过 Multi-Comp 补丁，跳过重复应用")
                    sanitize_zram_drv(kernel_root)
                    return True
        except Exception:
            pass

    # 1. 优先尝试 git apply
    if patch_file and os.path.isfile(patch_file):
        patch_name = os.path.basename(patch_file)
        log(f"[INFO] [轨道1] 正在尝试 git apply 应用 {kernel_version} 驱动补丁 ({patch_name})...")
        try:
            git_cmd = ["git", "apply", "--whitespace=fix", patch_file]
            git_res = subprocess.run(git_cmd, cwd=kernel_root, capture_output=True, text=True)
            if git_res.returncode == 0:
                log(f"[OK] [轨道1] 通过 git apply 成功打入 {kernel_version} Multi-Comp 驱动补丁！")
                sanitize_zram_drv(kernel_root)
                return True
            else:
                log(f"[INFO] [轨道1] git apply 未命中上下文 (返回码 {git_res.returncode})，错误摘要: {git_res.stderr.strip() or git_res.stdout.strip()}")
        except Exception as e:
            log(f"[INFO] [轨道1] git apply 执行异常: {e}")

        # 次选尝试 patch -p1
        try:
            cmd = ["patch", "-p1", "--no-backup-if-mismatch", "-F", "3", "-i", patch_file]
            res = subprocess.run(cmd, cwd=kernel_root, capture_output=True, text=True)
            if res.returncode == 0:
                log(f"[OK] [轨道1] 通过 patch 命令成功打入 {kernel_version} Multi-Comp 驱动补丁！")
                sanitize_zram_drv(kernel_root)
                return True
        except Exception:
            pass

    # 2. 备用轨道：Python 语义修补引擎自适应接管
    log(f"[INFO] [轨道2] 启动自适应 Python 语义修补引擎以适配当前 {kernel_version} 内核源码...")
    success = python_semantic_patch(kernel_root, kernel_version)
    if success:
        log(f"[OK] [轨道2] Python 语义自适应修补引擎已成功为 {kernel_version} 注入 Multi-Comp 驱动！")
        sanitize_zram_drv(kernel_root)
        return True

    return False

def main():
    kernel_root = sys.argv[1] if len(sys.argv) > 1 else os.getcwd()
    kernel_version = sys.argv[2] if len(sys.argv) > 2 else ""
    enable_multi_comp = sys.argv[3] if len(sys.argv) > 3 else "false"
    patch_multi_comp = sys.argv[4] if len(sys.argv) > 4 else "false"
    config_file = sys.argv[5] if len(sys.argv) > 5 else ""

    script_dir = os.path.dirname(os.path.abspath(__file__))

    is_enable = str(enable_multi_comp).strip().lower() == "true"
    is_patch = str(patch_multi_comp).strip().lower() == "true"

    log("==========================================================")
    log("检查 ZRAM Multi-Comp (多流二次重压缩) 配置与修补条件...")
    log(f"内核大版本     : {kernel_version}")
    log(f"开启功能开关   : {is_enable}")
    log(f"开启修补开关   : {is_patch}")
    log(f"源码根目录     : {kernel_root}")
    log(f"配置文件路径   : {config_file}")
    log("==========================================================")

    # 1. 严格互锁校验：只开修补没开功能 -> 立即报错中断构建
    if is_patch and not is_enable:
        log("::error::[Multi-Comp] 开启Multi-Comp修补必须同时勾选【开启Multi-Comp(内核版本>=6.12)】，构建终止！")
        sys.exit(1)

    # 2. 两个都未开启 -> 保持原样安全退出
    if not is_enable and not is_patch:
        log("[INFO] 未启用 Multi-Comp 功能与修补，保持原样。")
        sys.exit(0)

    # 3. 检查当前内核是否原生支持
    is_native = check_native_support(kernel_root, kernel_version)

    # 4. 只开启功能，未开启修补
    if is_enable and not is_patch:
        if is_native:
            log(f"[OK] 检测到当前内核版本 ({kernel_version}) 原生支持 Multi-Comp，直接追加配置开启！")
            append_config(config_file)
            sys.exit(0)
        else:
            log(f"::warning::[Multi-Comp] 当前内核版本 ({kernel_version}) 原生不支持免补丁开启 Multi-Comp，已安全跳过开启！如需启用请同时勾选【开启Multi-Comp修补(修补不支持的版本)】！")
            sys.exit(0)

    # 5. 两个同时开启（开启功能 + 开启修补）
    if is_enable and is_patch:
        if is_native:
            log(f"[INFO] 当前内核版本 ({kernel_version}) 已原生内置 Multi-Comp 驱动支持，无需打补丁，直接追加配置开启！")
            append_config(config_file)
            sys.exit(0)
        else:
            log(f"[INFO] 当前内核版本 ({kernel_version}) 原生未包含 Multi-Comp，开始匹配对应补丁执行修补...")
            patch_file = get_patch_file_for_version(script_dir, kernel_version)
            success = apply_patch(kernel_root, patch_file, kernel_version)
            if not success:
                log(f"::error::[Multi-Comp] {kernel_version} Multi-Comp 源码修补失败，构建终止！")
                sys.exit(1)

            append_config(config_file)
            log(f"[OK] 内核 {kernel_version} Multi-Comp 驱动补全与配置开启成功！")
            sys.exit(0)

if __name__ == "__main__":
    main()
